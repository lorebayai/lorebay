#!/usr/bin/env python3
import json
import tempfile
from pathlib import Path

from helpers import HERE, render, run

compose = run(
    "docker", "compose", "--env-file", HERE.parent / "docker/example.env",
    "-f", HERE.parent / "docker/compose.yml", "config", "--format", "json",
    capture_output=True,
)
IMAGE = json.loads(compose.stdout)["services"]["nginx"]["image"]


for source in HERE.parent.rglob("*.py"):
    compile(source.read_bytes(), str(source), "exec")
with tempfile.TemporaryDirectory(prefix="lorebay-nginx-") as temporary:
    directory = Path(temporary)
    run(
        "openssl",
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "1",
        "-subj",
        "/CN=lorebay.example.com",
        "-addext",
        "subjectAltName=DNS:lorebay.example.com",
        "-keyout",
        directory / "key.pem",
        "-out",
        directory / "certificate.pem",
        capture_output=True,
    )
    (directory / "site.conf").write_text(render("lorebay.example.com", "/check"))
    (directory / "live").symlink_to(".")
    (directory / "nginx.conf").write_text(
        "pid /tmp/nginx.pid;\nerror_log stderr;\nevents {}\nhttp {\n"
        "access_log off;\ninclude /check/site.conf;\n"
        "server { listen 443 ssl default_server; listen [::]:443 ssl default_server;"
        " ssl_reject_handshake on; }\n}\n"
    )
    run(
        "docker",
        "run",
        "--rm",
        "--network",
        "none",
        "--entrypoint",
        "nginx",
        "--mount",
        f"type=bind,source={directory},target=/check,readonly",
        IMAGE,
        "-t",
        "-c",
        "/check/nginx.conf",
    )
run("sh", "-n", HERE.parent / "docker/start-nginx.sh")
print("Deployment Python syntax and pinned nginx configuration checks passed")
