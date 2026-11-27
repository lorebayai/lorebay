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
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "common"))

from helpers import (
    TLS,
    atomic_write,
    certificate,
    copy_pair,
    domain_name,
    fail,
    input_path,
    lock,
    nginx_files,
    render,
    run,
    served_certificate,
    without_comments,
)

HERE = Path(__file__).resolve().parent
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


def trusted_metadata(metadata, label, path):
    if metadata.st_uid != 0 or metadata.st_mode & 0o022:
        fail(
            f"{label} must be owned by root and not writable by group or others: {path}"
        )


def trusted_path(path, label):
    for part in (path, *path.parents):
        trusted_metadata(part.stat(), label, part)


def inputs(args):
    for path in (args.users, args.certificate, args.key):
        if not path.is_file():
            fail(f"Required input file is missing: {path}")
    trusted_path(args.users, "Users file and its ancestors")
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
    for ancestor in path.parents:
        if not ancestor.exists():
            continue
        metadata = ancestor.stat()
        if metadata.st_uid != 0 or metadata.st_mode & 0o022:
            fail(
                f"Data directory parent must be owned by root and not writable by group or others: {ancestor}"
            )
        if not stat.S_ISDIR(metadata.st_mode) or not metadata.st_mode & stat.S_IXOTH:
            fail(
                f"Data directory parent is not traversable by the service user: {ancestor}"
            )
    if path.exists():
        if not path.is_dir():
            fail("Data path is not a directory")
        if any(path.iterdir()):
            fail(
                "Data directory is not empty; databases this installer did not create are refused"
            )
        metadata = path.stat()
        if metadata.st_uid != 0 or metadata.st_mode & 0o022:
            fail(
                f"Empty data directory must be owned by root and not writable by group or others: {path}"
            )
    parent = path
    while not parent.exists():
        parent = parent.parent
    if not parent.is_dir() or os.statvfs(parent).f_flag & os.ST_RDONLY:
        fail("Data directory cannot be created on a writable filesystem")
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
    if args.archives:
        trusted_path(args.archives, "Local archives directory and its ancestors")
    for name in (ARCHIVE, "SHA256SUMS"):
        destination = directory / name
        if args.archives:
            source = args.archives / name
            try:
                descriptor = os.open(
                    source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
                )
            except OSError:
                fail(
                    f"Local release input must be a readable regular file without symlinks: {source}"
                )
            with os.fdopen(descriptor, "rb") as handle:
                metadata = os.fstat(handle.fileno())
                if not stat.S_ISREG(metadata.st_mode):
                    fail(f"Local release input must be a regular file: {source}")
                trusted_metadata(metadata, "Local release input", source)
                with destination.open("wb") as target:
                    shutil.copyfileobj(handle, target)
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
    account = pwd.getpwnam("nobody")
    # The private release directory must stay inaccessible to the version-check user.
    with (directory / "lorebayd").open("rb") as executable:
        version = run(
            f"/proc/self/fd/{executable.fileno()}",
            "--version",
            user=account.pw_uid,
            group=account.pw_gid,
            extra_groups=(),
            pass_fds=(executable.fileno(),),
            cwd="/",
            env={},
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
        ).stdout.strip()
    if version != f"lorebayd {args.version.removeprefix('v')}":
        fail("Archive executable does not match the selected version")


def nginx_preflight(domain):
    files = nginx_files(["nginx"])
    text = "\n".join(files.values())
    clean = without_comments(text)
    if not re.search(r"include\s+/etc/nginx/conf.d/\*\.conf\s*;", clean):
        fail("nginx must include /etc/nginx/conf.d/*.conf inside http")
    names = re.findall(r"\bserver_name\s+([^;]+);", clean)
    if "lorebay_" in clean or any(
        domain in [token.strip("\"'") for token in directive.split()]
        for directive in names
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
        + "\n    access_log off;\n    ssl_reject_handshake on;\n}\n"
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
        created_data = []
        original_data = args.data.stat() if args.data.exists() else None
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
                HERE.parent,
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
                    created_data.append(parent)
                    os.chown(parent, 0, 0)
                    parent.chmod(0o755)
            if original_data is None:
                args.data.mkdir(mode=0o700)
                created_data.append(args.data)
            args.data.chmod(0o700)
            os.chown(args.data, account.pw_uid, account.pw_gid)
            if any(args.data.iterdir()):
                fail("Data directory is not empty; refusing an existing database")
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
            atomic_write(CONFIG / "lorebayd.json", json.dumps(config, indent=2) + "\n")
            CONFIG.chmod(0o755)
            service = (HERE / "lorebay.service").read_text()
            atomic_write(
                UNIT,
                service.replace("@USERS@", str(args.users)).replace(
                    "@DATA@", str(args.data)
                ),
            )
            atomic_write(SITE, render(args.domain))
            if reject:
                atomic_write(REJECT, reject)
            run("nginx", "-t")
            run("systemctl", "daemon-reload")
            run("systemctl", "enable", "--now", "lorebay.service", "nginx.service")
            run("systemctl", "reload", "nginx.service")
            served_certificate(TLS / "live/certificate.pem", args.domain)
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
            atomic_write(state, json.dumps(wanted, indent=2) + "\n", mode=0o600)
            activated = True
            print(
                "Lorebay installed; use systemctl status lorebay and journalctl -u lorebay"
            )
        finally:
            if not activated:
                run("systemctl", "disable", "--now", "lorebay.service", check=False)
                for path in (SITE, REJECT, UNIT):
                    path.unlink(missing_ok=True)
                if args.data in created_data:
                    shutil.rmtree(args.data)
                    created_data.pop()
                elif original_data is not None:
                    os.chown(args.data, original_data.st_uid, original_data.st_gid)
                    args.data.chmod(stat.S_IMODE(original_data.st_mode))
                    if any(args.data.iterdir()):
                        print(
                            f"Data directory retained; clear it before retrying: {args.data}"
                        )
                for parent in reversed(created_data):
                    parent.rmdir()
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
    saved = {path: path.read_text() for path in (SITE, REJECT) if path.exists()}
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
            atomic_write(path, content)
        raise
    run("systemctl", "disable", "--now", "lorebay.service")
    installed = json.loads((CONFIG / "install.json").read_text())
    run("chown", "--recursive", "--no-dereference", "root:root", installed["data"])
    run("userdel", "lorebay")
    UNIT.unlink()
    run("systemctl", "daemon-reload")
    for path in (CONFIG, TLS, INSTALL_DIRECTORY):
        shutil.rmtree(path)
    print(
        "Lorebay removed; nginx, its other sites, root-owned data and supplied files remain"
    )


def main():
    parser = argparse.ArgumentParser(
        description="Install a fresh Lorebay service or remove its managed files"
    )
    commands = parser.add_subparsers(dest="action", required=True)
    add = commands.add_parser("install")
    add.add_argument("--version", default="v0.1.4")
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
        fail("Supply a release tag such as v0.1.4")
    with lock(Path("/run/lock/lorebay-install.lock")):
        install(args) if args.action == "install" else uninstall()


if __name__ == "__main__":
    main()
