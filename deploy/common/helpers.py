import contextlib
import fcntl
import fnmatch
import os
import re
import shutil
import socket
import ssl
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
TLS = Path("/etc/nginx/lorebay-tls")


def run(*args, **kwargs):
    kwargs.setdefault("check", True)
    return subprocess.run([str(a) for a in args], **kwargs)


def output(*args):
    return run(*args, capture_output=True, text=True).stdout.strip()


def fail(message):
    raise SystemExit(message)


def atomic_write(path, content, mode=0o644):
    with tempfile.NamedTemporaryFile(
        mode="w", dir=path.parent, prefix=f".{path.name}-", delete=False
    ) as handle:
        try:
            handle.write(content)
            handle.flush()
            os.fchmod(handle.fileno(), mode)
            os.replace(handle.name, path)
        finally:
            Path(handle.name).unlink(missing_ok=True)


def domain_name(value):
    if len(value) > 253 or not re.fullmatch(
        r"(?=.{1,253}$)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+"
        r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?",
        value,
    ):
        fail("Supply a lowercase DNS domain without a port or scheme")
    return value


def input_path(value):
    path = Path(value)
    resolved = path.resolve()
    if not path.is_absolute() or any(
        not re.fullmatch(r"/[A-Za-z0-9_./-]+", candidate)
        for candidate in (value, str(resolved))
    ):
        fail(
            "Input and resolved paths must be absolute and contain only letters, digits, /, ., _, -"
        )
    return resolved


@contextlib.contextmanager
def lock(path):
    with path.open("a") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def certificate(certificate_path, key_path, domain):
    run(
        "openssl",
        "x509",
        "-in",
        certificate_path,
        "-noout",
        "-checkend",
        "0",
        "-checkhost",
        domain,
        stdout=subprocess.DEVNULL,
    )
    run(
        "openssl",
        "pkey",
        "-in",
        key_path,
        "-passin",
        "pass:",
        "-check",
        "-noout",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    cert_key = output("openssl", "x509", "-in", certificate_path, "-pubkey", "-noout")
    private_key = output(
        "openssl", "pkey", "-in", key_path, "-passin", "pass:", "-pubout"
    )
    if cert_key != private_key:
        fail("Certificate and private key do not match")
    # x509 -checkhost reports a mismatch on stdout without a failing exit status.
    if "does match certificate" not in output(
        "openssl", "x509", "-in", certificate_path, "-noout", "-checkhost", domain
    ):
        fail("Certificate does not cover the domain")


def copy_pair(cert, key, directory):
    directory.mkdir(mode=0o700)
    for source, name in ((cert, "certificate.pem"), (key, "key.pem")):
        destination = directory / name
        shutil.copyfile(source, destination)
        destination.chmod(0o600)


def served_certificate(cert, domain, connect="127.0.0.1"):
    leaf = re.search(
        r"-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----",
        cert.read_text(),
        re.S,
    ).group()
    expected = ssl.PEM_cert_to_DER_cert(leaf)
    context = ssl._create_unverified_context()
    last_error = None
    # Reload returns before nginx's master accepts the new configuration.
    for _ in range(50):
        try:
            with socket.create_connection((connect, 443), timeout=2) as connection:
                with context.wrap_socket(connection, server_hostname=domain) as client:
                    if client.getpeercert(binary_form=True) == expected:
                        return
        except OSError as error:
            last_error = error
        time.sleep(0.1)
    fail(f"nginx did not serve the staged certificate: {last_error}")


def render(domain, tls=TLS, upstream="127.0.0.1:8080", upload_limit=5000000000):
    text = (HERE / "nginx.conf.in").read_text()
    for name, value in {
        "DOMAIN": domain,
        "TLS": tls,
        "UPSTREAM": upstream,
        "UPLOAD_LIMIT": upload_limit,
        "ACCESS_LOG": "",
    }.items():
        text = text.replace(f"@{name}@", str(value))
    return text


def nginx_files(command):
    dump = output(*command, "-T")
    sections = re.split(r"^# configuration file (.+):\n", dump, flags=re.M)
    files = dict(zip(sections[1::2], sections[2::2]))
    if not files:
        fail("nginx -T did not expose its configuration files")
    return files


def without_comments(text):
    return re.sub(
        r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|#[^\n]*',
        lambda m: "" if m[0].startswith("#") else m[0],
        text,
    )


def staged_nginx(command, directory, visible_directory, tls, staged):
    files = nginx_files(command)
    root = next(iter(files))
    directory.mkdir(mode=0o700)
    names = {
        name: str(visible_directory / str(index)) for index, name in enumerate(files)
    }
    replacements = 0
    for index, content in enumerate(files.values()):

        def include(match):
            pattern = match[1].strip("\"'")
            if not pattern.startswith("/"):
                pattern = str(Path(root).parent / pattern)
            matches = [names[n] for n in files if fnmatch.fnmatchcase(n, pattern)]
            if not matches and not any(c in pattern for c in "*?["):
                fail(f"Cannot stage nginx include: {pattern}")
            return "\n".join(f"include {n};" for n in matches)

        content = without_comments(content)
        content = re.sub(r"\binclude\s+([^;\s]+)\s*;", include, content)
        replacements += content.count(f"{tls}/live/")
        content = content.replace(f"{tls}/live/", f"{tls}/{staged}/")
        (directory / str(index)).write_text(content)
    if replacements < 2:
        fail("nginx must reference the managed live certificate and key")
    run(*command, "-t", "-c", names[root])
