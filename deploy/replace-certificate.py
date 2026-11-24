#!/usr/bin/env python3
import argparse
import os
import shutil
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
    served_certificate,
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
        if Path(previous).name != previous or previous in (".", ".."):
            fail("The live certificate must name a pair inside the managed directory")
        switched = False
        try:
            copy_pair(args.certificate, args.key, staged)
            certificate(staged / "certificate.pem", staged / "key.pem", args.domain)
            staged_nginx(command, validation, visible / validation.name, visible, name)
            (directory / "next").symlink_to(name)
            os.replace(directory / "next", directory / "live")
            switched = True
            run(*command, "-t")
            run(*command, "-s", "reload")
            served_certificate(staged / "certificate.pem", args.domain, args.connect)
        except BaseException:
            if switched:
                (directory / "next").symlink_to(previous)
                os.replace(directory / "next", directory / "live")
                run(*command, "-t")
                run(*command, "-s", "reload")
                print("Previous certificate restored")
            raise
        finally:
            shutil.rmtree(validation, ignore_errors=True)
            if not switched or os.readlink(directory / "live") != name:
                shutil.rmtree(staged, ignore_errors=True)
        shutil.rmtree(directory / previous)
        print("Certificate activated and served; previous pair removed")


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
