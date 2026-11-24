#!/usr/bin/env python3
import argparse
import os
import re
import shutil
import socket
import ssl
import sys
import time
import uuid
from pathlib import Path

from common import (
    TLS,
    certificate,
    copy_pair,
    domain_name,
    fail,
    lock,
    run,
    staged_nginx,
)


def replace(args):
    directory = args.directory.resolve()
    visible = args.nginx_directory or (TLS if args.container else directory)
    command = (
        ["docker", "exec", args.container, "nginx"] if args.container else ["nginx"]
    )
    if not (directory / "live").is_symlink():
        fail("The certificate directory must have a managed live symlink")
    with lock(directory / ".lock"):
        name = f"pair-{uuid.uuid4().hex}"
        staged = directory / name
        validation = directory / f"check-{uuid.uuid4().hex}"
        previous = os.readlink(directory / "live")
        switched = False
        try:
            copy_pair(args.certificate, args.key, staged)
            certificate(staged / "certificate.pem", staged / "key.pem", args.domain)
            staged_nginx(command, validation, visible / validation.name, visible, name)
            leaf = re.search(
                r"-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----",
                (staged / "certificate.pem").read_text(),
                re.S,
            ).group()
            fingerprint = ssl.PEM_cert_to_DER_cert(leaf)
            (directory / "next").symlink_to(name)
            os.replace(directory / "next", directory / "live")
            switched = True
            run(*command, "-t")
            run(*command, "-s", "reload")
            context = ssl._create_unverified_context()
            last_error = None
            for _ in range(50):
                try:
                    with socket.create_connection(
                        (args.connect, 443), timeout=2
                    ) as connection:
                        with context.wrap_socket(
                            connection, server_hostname=args.domain
                        ) as client:
                            if client.getpeercert(binary_form=True) == fingerprint:
                                print("Certificate activated and served")
                                return
                except OSError as error:
                    last_error = error
                time.sleep(0.1)
            fail(f"nginx did not serve the staged certificate: {last_error}")
        finally:
            if switched and os.readlink(directory / "live") == name:
                # Reload can return before nginx's master accepts the new configuration.
                if sys.exc_info()[0] is not None:
                    (directory / "next").symlink_to(previous)
                    os.replace(directory / "next", directory / "live")
                    run(*command, "-t")
                    run(*command, "-s", "reload")
                    print("Previous certificate restored")
            shutil.rmtree(validation, ignore_errors=True)
            if not switched or os.readlink(directory / "live") != name:
                shutil.rmtree(staged, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(
        description="Validate, replace and activate nginx's certificate"
    )
    parser.add_argument("--domain", required=True, type=domain_name)
    parser.add_argument("--certificate", required=True, type=Path)
    parser.add_argument("--key", required=True, type=Path)
    parser.add_argument("--directory", type=Path, default=TLS)
    parser.add_argument(
        "--container", help="nginx container with the certificate directory mounted"
    )
    parser.add_argument(
        "--nginx-directory", type=Path, help="certificate directory inside nginx"
    )
    parser.add_argument(
        "--connect",
        default="127.0.0.1",
        help="nginx address for activation verification on port 443",
    )
    replace(parser.parse_args())


if __name__ == "__main__":
    main()
