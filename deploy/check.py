#!/usr/bin/env python3
import tempfile
from pathlib import Path

from common import HERE, render, run

IMAGE = "nginx:1.28-alpine@sha256:a8b39bd9cf0f83869a2162827a0caf6137ddf759d50a171451b335cecc87d236"


for source in HERE.glob("*.py"):
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
print("Deployment Python syntax and pinned nginx configuration checks passed")
