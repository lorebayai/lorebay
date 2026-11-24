import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = "ghcr.io/lorebayai/lorebayd"


def run(*args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, text=True, **kwargs)


def paths():
    target = os.environ["RELEASE_TARGET"]
    if target not in ("x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"):
        raise SystemExit(f"Unsupported image target: {target}")
    directory = ROOT / "target/release-images" / target
    directory.mkdir(parents=True, exist_ok=True)
    return target, directory, directory / "image"


def build():
    target, directory, stored = paths()
    architecture = {"x86_64": "amd64", "aarch64": "arm64"}[target.split("-")[0]]
    context = ROOT / "target/release-archives" / target / "lorebayd"
    image = f"lorebay-build-{uuid.uuid4().hex}:build"
    try:
        run("docker", "build", "--platform", f"linux/{architecture}",
            "--provenance=false", "--file", ROOT / "deploy/docker/Dockerfile", "--tag", image, context)
        with tempfile.TemporaryDirectory(dir=directory) as temporary:
            archive = Path(temporary) / "image.tar"
            run("docker", "image", "save", "--output", archive, image)
            if stored.exists():
                shutil.rmtree(stored)
            # Docker-format conversion retains HEALTHCHECK, unlike OCI conversion.
            run("skopeo", "copy", "--format", "v2s2", "--dest-compress",
                f"docker-archive:{archive}", f"dir:{stored}")
    finally:
        subprocess.run(["docker", "image", "rm", image], check=False)
    manifest = (stored / "manifest.json").read_bytes()
    (directory / "digest.txt").write_text(f"sha256:{hashlib.sha256(manifest).hexdigest()}\n")


def smoke():
    target, directory, stored = paths()
    if not target.startswith(platform.machine().replace("arm64", "aarch64")):
        raise SystemExit("Image smoke requires its native architecture")
    name = f"lorebay-smoke-{uuid.uuid4().hex}"
    image = f"{name}:smoke"
    try:
        run("skopeo", "copy", f"dir:{stored}", f"docker-daemon:{image}")
        binary = ROOT / "target" / target / "release/lorebayd"
        hashed = run(binary, "hash-password", input="smoke\n", capture_output=True).stdout.strip()
        with tempfile.TemporaryDirectory(dir=directory) as temporary:
            temporary = Path(temporary)
            temporary.chmod(0o755)
            users = temporary / "users.json"
            users.write_text(json.dumps([{"username": "smoke", "password_hash": hashed}]))
            users.chmod(0o444)
            run("docker", "run", "--detach", "--name", name, "--network", "none",
                "--tmpfs", "/data:uid=65532,gid=65532,mode=0700",
                "--mount", f"type=bind,source={users},target=/etc/lorebay/users.yaml,readonly",
                "--mount", f"type=bind,source={ROOT}/deploy/docker/lorebayd.json,target=/etc/lorebay/lorebayd.json,readonly", image)
            for _ in range(60):
                state = json.loads(run("docker", "inspect", name, capture_output=True).stdout)[0]["State"]
                if state.get("Health", {}).get("Status") == "healthy":
                    run("docker", "exec", name, "/usr/local/bin/lorebayd", "readiness", "--config", "/etc/lorebay/lorebayd.json")
                    print(f"Native readiness smoke passed: {target}; {directory.joinpath('digest.txt').read_text().strip()}")
                    return
                if not state["Running"]:
                    break
                time.sleep(1)
            run("docker", "logs", name)
            raise SystemExit("Image readiness smoke failed")
    finally:
        subprocess.run(["docker", "rm", "--force", name], check=False)
        subprocess.run(["docker", "image", "rm", image], check=False)


def setup():
    run("sudo", "apt-get", "update")
    run("sudo", "apt-get", "install", "-y", "skopeo")


def push():
    _, directory, stored = paths()
    digest = (directory / "digest.txt").read_text().strip()
    run("skopeo", "copy", "--preserve-digests", "--authfile", Path.home() / ".docker/config.json",
        f"dir:{stored}", f"docker://{REPOSITORY}@{digest}")


def manifest():
    directory = ROOT / "target/release-images"
    digests = sorted(directory.glob("*/digest.txt"))
    if len(digests) != 2:
        raise SystemExit("Both smoked image digests are required")
    number = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    reference = f"{REPOSITORY}:{number}"
    run("docker", "buildx", "imagetools", "create", "--tag", reference,
        *(f"{REPOSITORY}@{p.read_text().strip()}" for p in digests))
    result = run("docker", "buildx", "imagetools", "inspect", reference,
                 "--format", "{{json .Manifest}}", capture_output=True)
    digest = json.loads(result.stdout)["digest"]
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"digest={digest}\n")


if __name__ == "__main__":
    {"build": build, "smoke": smoke, "push": push, "manifest": manifest, "setup": setup}[sys.argv[1]]()
