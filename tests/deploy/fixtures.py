import hashlib
import json
import pwd
import sys
from pathlib import Path


def snapshot():
    items = {}
    paths = (
        "/etc/lorebay",
        "/opt/lorebay",
        "/etc/nginx/lorebay-tls",
        "/etc/systemd/system/lorebay.service",
        "/etc/nginx/conf.d",
        "/srv/data",
    )
    for name in paths:
        path = Path(name)
        children = sorted(path.rglob("*")) if path.is_dir() else []
        for entry in [path, *children]:
            if not entry.exists() and not entry.is_symlink():
                continue
            stat = entry.lstat()
            content = None
            if entry.is_symlink():
                content = entry.readlink().as_posix()
            elif entry.is_file():
                content = hashlib.sha256(entry.read_bytes()).hexdigest()
            items[str(entry)] = [stat.st_mode, stat.st_uid, stat.st_gid, content]
    items["user"] = [user.pw_name for user in pwd.getpwall() if user.pw_name == "lorebay"]
    print(json.dumps(items, sort_keys=True))


def tamper(path, archive):
    path = Path(path)
    lines = ["0" * 64 + line[64:] if line.endswith("  " + archive) else line for line in path.read_text().splitlines()]
    path.write_text("\n".join(lines) + "\n")


def hosts(domain, address):
    path = Path("/etc/hosts")
    lines = [line for line in path.read_text().splitlines() if domain not in line.split()[1:]]
    path.write_text("\n".join([*lines, address + " " + domain]) + "\n")


if __name__ == "__main__":
    {"snapshot": snapshot, "tamper": tamper, "hosts": hosts}[sys.argv[1]](*sys.argv[2:])
