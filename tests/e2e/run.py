import argparse
import getpass
import json
import os
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "common"))
from clients import redact, require
from journeys import JOURNEYS, run_journeys
from people import create_journey_people


WORKSPACE = Path(__file__).resolve().parents[2]


def arguments():
    parser = argparse.ArgumentParser(description="Run team journeys against a fresh checkout-built server")
    parser.add_argument("--server", type=Path, default=WORKSPACE / "target/debug/lorebayd")
    parser.add_argument("--cli", type=Path, default=WORKSPACE / "target/debug/lorebay")
    parser.add_argument("--chromium", help="Chromium executable; otherwise use Playwright's installed Chromium")
    parser.add_argument("--journey", type=int, choices=JOURNEYS, action="append",
                        help="Run only these journeys, in order")
    return parser.parse_args()


def write_private(path: Path, content: str):
    with open(path, "x", opener=lambda name, flags: os.open(name, flags, 0o600)) as output:
        output.write(content)


def create_people(server, numbers, credentials):
    people = create_journey_people("team-" + secrets.token_hex(6), numbers)
    users = []
    for group in people.values():
        for person in group:
            credentials.append(person["password"])
            result = subprocess.run(
                [str(server), "hash-password"], input=person["password"] + "\n",
                capture_output=True, text=True, timeout=30, check=False,
            )
            require(result.returncode == 0, "Password hashing failed")
            hashed = result.stdout.strip()
            credentials.append(hashed)
            users.append({"username": person["username"], "password_hash": hashed})
    return people, users


def serve(server, root, users, numbers, credentials, options):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    write_private(root / "users.json", json.dumps(users))
    write_private(root / "config.json", json.dumps({
        "listen_address": f"127.0.0.1:{port}", "public_url": origin,
        "data_directory": str(root / "data"), "users_file": str(root / "users.json"),
        "login_rate_limit": 200, "registration_rate_limit": 100, "device_poll_interval_seconds": 1,
        "logging": {"level": "warn"},
    }))
    environment = {key: value for key, value in os.environ.items() if not key.startswith("LOREBAYD__")}
    child = subprocess.Popen(
        [str(server), "--config", str(root / "config.json")], cwd=root, env=environment,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    assert child.stdout is not None and child.stderr is not None
    logs = []

    def collect(stream):
        for line in stream:
            logs.append(line)

    readers = [threading.Thread(target=collect, args=(stream,), daemon=True) for stream in (child.stdout, child.stderr)]
    for reader in readers:
        reader.start()
    try:
        deadline = time.monotonic() + 30
        while True:
            require(child.poll() is None, "Server exited before readiness")
            try:
                with urllib.request.urlopen(origin + "/readyz", timeout=1) as response:
                    if response.status == 200:
                        break
            except (OSError, urllib.error.URLError):
                pass
            require(time.monotonic() < deadline, "Server readiness did not arrive")
            time.sleep(0.1)
        run_journeys(origin, people=options.people, root=root / "clients", executable=options.cli.resolve(),
                     chromium=options.chromium, numbers=numbers, credentials=credentials)
    finally:
        if child.poll() is None:
            child.terminate()
        try:
            child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=10)
        for reader in readers:
            reader.join(timeout=5)
        child.stdout.close()
        child.stderr.close()
        raw = "".join(logs)
        require(not any(credential in raw for credential in credentials), "Server logs contain a generated credential")
        require(child.returncode == 0, "Server did not stop cleanly: " + redact(raw, credentials))


def main():
    options = arguments()
    numbers = list(dict.fromkeys(options.journey or JOURNEYS))
    credentials = []
    created = []
    started = time.monotonic()
    handlers = {}

    def interrupt(_signum, _frame):
        raise InterruptedError("Run interrupted")

    try:
        for signum in (signal.SIGINT, signal.SIGTERM):
            handlers[signum] = signal.signal(signum, interrupt)
        server = options.server.resolve(strict=True)
        options.cli = options.cli.resolve(strict=True)
        base = Path(tempfile.gettempdir()) / getpass.getuser() / "lorebay"
        for directory in (base.parent, base):
            if not directory.exists():
                directory.mkdir(mode=0o700)
                created.append(directory)
        with tempfile.TemporaryDirectory(prefix="e2e-", dir=base) as temporary:
            root = Path(temporary)
            options.people, users = create_people(server, numbers, credentials)
            serve(server, root, users, numbers, credentials, options)
        require(not root.exists(), "Run directory remains after cleanup")
        duration = time.monotonic() - started
        print(f"PASS {len(numbers)} journeys; server stopped; temporary files removed ({duration:.1f}s)")
        return 0
    except Exception as error:
        print("FAIL " + redact(str(error), credentials), flush=True)
        return 1
    finally:
        for signum, handler in handlers.items():
            signal.signal(signum, handler)
        for directory in reversed(created):
            directory.rmdir()


if __name__ == "__main__":
    sys.exit(main())
