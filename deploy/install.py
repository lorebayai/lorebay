#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import platform
import pwd
import re
import shutil
import socket
import stat
import tarfile
import tempfile
from pathlib import Path

from common import (
    HERE,
    TLS,
    certificate,
    copy_pair,
    domain_name,
    fail,
    input_path,
    lock,
    nginx_files,
    output,
    render,
    run,
    without_comments,
)

CONFIG = Path("/etc/lorebay")
INSTALL_DIRECTORY = Path("/opt/lorebay")
UNIT = Path("/etc/systemd/system/lorebay.service")
SITE = Path("/etc/nginx/conf.d/lorebay.conf")
REJECT = Path("/etc/nginx/conf.d/lorebay-reject.conf")
ARCHIVE = "lorebayd-x86_64-unknown-linux-musl.tar.gz"


def host():
    release = dict(
        line.split("=", 1)
        for line in Path("/etc/os-release").read_text().splitlines()
        if "=" in line
    )
    identity = tuple(release.get(key, "").strip('"') for key in ("ID", "VERSION_ID"))
    if platform.machine() != "x86_64" or identity not in (
        ("debian", "12"),
        ("ubuntu", "24.04"),
    ):
        fail("Supported hosts: x86_64 Debian 12 or Ubuntu 24.04 only")
    if Path("/proc/1/comm").read_text().strip() != "systemd":
        fail("systemd must run as PID 1")
    if os.geteuid() != 0:
        fail("Run as root")


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def inputs(args):
    for path in (args.users, args.certificate, args.key):
        if not path.is_file():
            fail(f"Required input file is missing: {path}")
    certificate(args.certificate, args.key, args.domain)
    return {
        "version": args.version,
        "domain": args.domain,
        "data": str(args.data),
        "files": {str(p): digest(p) for p in (args.users, args.certificate, args.key)},
    }


def data_directory(path):
    for temporary in (Path("/tmp"), Path("/var/tmp")):
        if path == temporary or temporary in path.parents:
            fail(
                "Data directory must be outside /tmp and /var/tmp; the service uses private temporary directories"
            )
    if path.exists():
        if not path.is_dir():
            fail("Data path is not a directory")
        if any(path.iterdir()):
            fail(
                "Data directory is not empty; databases this installer did not create are refused"
            )
    parent = path
    while not parent.exists():
        parent = parent.parent
    if not parent.is_dir() or os.statvfs(parent).f_flag & os.ST_RDONLY:
        fail("Data directory cannot be created on a writable filesystem")
    if parent == path:
        parent = parent.parent
    for ancestor in [parent, *parent.parents]:
        if not ancestor.stat().st_mode & stat.S_IXOTH:
            fail(
                f"Data directory parent is not traversable by the service user: {ancestor}"
            )
    for process in Path("/proc").glob("[0-9]*"):
        try:
            if Path(os.readlink(process / "exe")).name != "lorebayd":
                continue
            for descriptor in (process / "fd").iterdir():
                target = descriptor.resolve()
                if target == path or path in target.parents:
                    fail(
                        f"Another lorebayd is using the data directory (PID {process.name})"
                    )
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue


def release(args, directory):
    for name in (ARCHIVE, "SHA256SUMS"):
        destination = directory / name
        if args.archives:
            shutil.copyfile(args.archives / name, destination)
        else:
            base = (
                args.release_url
                or f"https://github.com/lorebayai/lorebay/releases/download/{args.version}"
            )
            if not base.startswith("https://"):
                fail("Release downloads require HTTPS")
            run(
                "curl",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--output",
                destination,
                f"{base.rstrip('/')}/{name}",
            )
    matches = re.findall(
        rf"^([a-fA-F0-9]{{64}}) [ *]{re.escape(ARCHIVE)}$",
        (directory / "SHA256SUMS").read_text(),
        flags=re.M,
    )
    if len(matches) != 1 or digest(directory / ARCHIVE) != matches[0].lower():
        fail("Release SHA256SUMS checksum mismatch or missing archive entry")
    with tarfile.open(directory / ARCHIVE) as archive:
        for name in ("lorebayd", "LICENSE", "THIRD-PARTY-NOTICES.txt"):
            members = [
                member for member in archive if member.name == name and member.isfile()
            ]
            if len(members) != 1:
                fail(f"Release archive must contain one regular {name}")
            with (
                archive.extractfile(members[0]) as source,
                (directory / name).open("wb") as target,
            ):
                shutil.copyfileobj(source, target)
    (directory / "lorebayd").chmod(0o755)
    if (
        output(directory / "lorebayd", "--version")
        != f"lorebayd {args.version.removeprefix('v')}"
    ):
        fail("Archive executable does not match the selected version")


def nginx_preflight(domain):
    files = nginx_files(["nginx"])
    text = "\n".join(files.values())
    clean = without_comments(text)
    if not re.search(r"include\s+/etc/nginx/conf.d/\*\.conf\s*;", clean):
        fail("nginx must include /etc/nginx/conf.d/*.conf inside http")
    if "lorebay_" in clean or re.search(
        rf"\bserver_name\s+[^;]*\b{re.escape(domain)}\b", clean
    ):
        fail("nginx already contains a conflicting Lorebay zone or domain")
    defaults = set()
    for address, options in re.findall(r"\blisten\s+([^;\s]+)([^;]*);", clean):
        if not re.search(r"(?:^|:)443$", address):
            continue
        if address not in ("443", "0.0.0.0:443", "[::]:443"):
            fail(
                "Address-specific nginx listeners on port 443 need manual configuration"
            )
        if "default_server" in options:
            defaults.add("ipv6" if address == "[::]:443" else "ipv4")
    listeners = []
    if "ipv4" not in defaults:
        listeners.append("listen 443 ssl default_server;")
    if "ipv6" not in defaults:
        listeners.append("listen [::]:443 ssl default_server;")
    if not listeners:
        return ""
    return (
        "server {\n    "
        + "\n    ".join(listeners)
        + "\n    ssl_reject_handshake on;\n}\n"
    )


def install(args):
    wanted = inputs(args)
    state = CONFIG / "install.json"
    if state.exists():
        installed = json.loads(state.read_text())
        if installed["version"] != args.version:
            fail("Another version is installed; upgrades are not supported")
        if installed != wanted:
            fail(
                "Installation inputs differ; only an identical repeated install is supported"
            )
        print("Same version and inputs are already installed; nothing changed")
        return
    for path in (CONFIG, INSTALL_DIRECTORY, UNIT, SITE, REJECT, TLS):
        if path.exists() or path.is_symlink():
            fail(
                f"Unmanaged or another installed version of Lorebay at {path}; refusing to replace it"
            )
    for name in ("lorebayd",):
        if shutil.which(name):
            fail(
                f"Another installed version ({name}) exists; upgrades are not supported"
            )
    if any(p.pw_name == "lorebay" for p in pwd.getpwall()):
        fail("An unmanaged lorebay account already exists")
    for managed in (CONFIG, INSTALL_DIRECTORY, TLS):
        if args.data == managed or managed in args.data.parents:
            fail(
                "Data directory must be outside the managed configuration and executable directories"
            )
    data_directory(args.data)
    with socket.socket() as listener:
        try:
            listener.bind(("127.0.0.1", 8080))
        except OSError:
            fail("Loopback port 8080 is already in use")
    with tempfile.TemporaryDirectory(prefix="lorebay-install-") as temporary:
        directory = Path(temporary)
        release(args, directory)
        if not shutil.which("nginx"):
            run("apt-get", "update")
            run("apt-get", "install", "-y", "--no-upgrade", "nginx")
        reject = nginx_preflight(args.domain)
        activated = False
        user_created = False
        try:
            CONFIG.mkdir(mode=0o700)
            INSTALL_DIRECTORY.mkdir(mode=0o755)
            INSTALL_DIRECTORY.chmod(0o755)
            TLS.mkdir(mode=0o700)
            copy_pair(args.certificate, args.key, TLS / "initial")
            (TLS / "live").symlink_to("initial")
            for name in ("lorebayd", "LICENSE", "THIRD-PARTY-NOTICES.txt"):
                shutil.copy2(directory / name, INSTALL_DIRECTORY / name)
            shutil.copytree(
                HERE,
                INSTALL_DIRECTORY / "deploy",
                ignore=shutil.ignore_patterns("__pycache__"),
            )
            run(
                "useradd",
                "--system",
                "--user-group",
                "--home-dir",
                INSTALL_DIRECTORY,
                "--no-create-home",
                "--shell",
                "/usr/sbin/nologin",
                "lorebay",
            )
            user_created = True
            account = pwd.getpwnam("lorebay")
            for parent in reversed(args.data.parents):
                if not parent.exists():
                    parent.mkdir(mode=0o755)
                    parent.chmod(0o755)
            args.data.mkdir(exist_ok=True, mode=0o700)
            args.data.chmod(0o700)
            os.chown(args.data, account.pw_uid, account.pw_gid)
            config = {
                "public_url": f"https://{args.domain}",
                "data_directory": str(args.data),
                "listen_address": "127.0.0.1:8080",
                "users_file": "/run/credentials/lorebay.service/users",
                "login_rate_limit": 1000,
                "registration_rate_limit": 1000,
                "device_rate_limit": 1000,
                "device_verification_rate_limit": 1000,
            }
            # The TCP peer is nginx, so application budgets cover the whole team.
            (CONFIG / "lorebayd.json").write_text(json.dumps(config, indent=2) + "\n")
            CONFIG.chmod(0o755)
            (CONFIG / "lorebayd.json").chmod(0o644)
            service = (HERE / "lorebay.service").read_text()
            UNIT.write_text(
                service.replace("@USERS@", str(args.users)).replace(
                    "@DATA@", str(args.data)
                )
            )
            SITE.write_text(render(args.domain))
            if reject:
                REJECT.write_text(reject)
            run("nginx", "-t")
            run("systemctl", "daemon-reload")
            run("systemctl", "enable", "--now", "lorebay.service", "nginx.service")
            run("systemctl", "reload", "nginx.service")
            run(
                "curl",
                "--fail",
                "--silent",
                "--show-error",
                "--retry",
                "15",
                "--retry-connrefused",
                "--retry-delay",
                "1",
                "http://127.0.0.1:8080/readyz",
            )
            state.write_text(json.dumps(wanted, indent=2) + "\n")
            state.chmod(0o600)
            activated = True
            print(
                "Lorebay installed; use systemctl status lorebay and journalctl -u lorebay"
            )
        finally:
            if not activated:
                run("systemctl", "disable", "--now", "lorebay.service", check=False)
                for path in (SITE, REJECT, UNIT):
                    path.unlink(missing_ok=True)
                if user_created:
                    run("userdel", "lorebay")
                for path in (CONFIG, INSTALL_DIRECTORY, TLS):
                    shutil.rmtree(path, ignore_errors=True)
                run("systemctl", "daemon-reload")
                run("nginx", "-t")
                if (
                    run(
                        "systemctl", "is-active", "--quiet", "nginx", check=False
                    ).returncode
                    == 0
                ):
                    run("systemctl", "reload", "nginx")


def uninstall():
    if not (CONFIG / "install.json").exists():
        fail("No managed installation exists")
    saved = {path: path.read_bytes() for path in (SITE, REJECT) if path.exists()}
    try:
        for path in saved:
            path.unlink()
        run("nginx", "-t")
        if (
            run("systemctl", "is-active", "--quiet", "nginx", check=False).returncode
            == 0
        ):
            run("systemctl", "reload", "nginx")
    except BaseException:
        for path, content in saved.items():
            path.write_bytes(content)
        raise
    run("systemctl", "disable", "--now", "lorebay.service")
    UNIT.unlink()
    run("systemctl", "daemon-reload")
    for path in (CONFIG, TLS, INSTALL_DIRECTORY):
        shutil.rmtree(path)
    # Retain the account so preserved data keeps its numeric owner across uninstall.
    print(
        "Lorebay removed; nginx, its other sites, the service account, data and supplied files remain"
    )


def main():
    parser = argparse.ArgumentParser(
        description="Install a fresh Lorebay service or remove its managed files"
    )
    commands = parser.add_subparsers(dest="action", required=True)
    add = commands.add_parser("install")
    add.add_argument("--version", default="v0.1.1")
    add.add_argument("--domain", type=domain_name, required=True)
    add.add_argument("--users", type=input_path, required=True)
    add.add_argument("--certificate", type=input_path, required=True)
    add.add_argument("--key", type=input_path, required=True)
    add.add_argument("--data", type=input_path, required=True)
    source = add.add_mutually_exclusive_group()
    source.add_argument("--archives", type=input_path)
    source.add_argument(
        "--release-url",
        help="HTTPS directory containing the selected archive and SHA256SUMS",
    )
    commands.add_parser("uninstall")
    args = parser.parse_args()
    host()
    if args.action == "install" and not re.fullmatch(
        r"v[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?", args.version
    ):
        fail("Supply a release tag such as v0.1.1")
    with lock(Path("/run/lock/lorebay-install.lock")):
        install(args) if args.action == "install" else uninstall()


if __name__ == "__main__":
    main()
