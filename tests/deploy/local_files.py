import hashlib
import tarfile
from pathlib import Path

from artifacts import NATIVE_ARCHIVES, TARGET


ROOT = Path(__file__).resolve().parents[2]


def metadata(member):
    member.uid = member.gid = 0
    member.uname = member.gname = ""
    member.mode = 0o755 if member.isdir() or member.mode & 0o111 else 0o644
    return member


def main():
    directory = ROOT / "target/release-archives" / TARGET
    for name in NATIVE_ARCHIVES:
        if not (directory / name).is_file():
            raise SystemExit(f"Missing {name}; run the native release build and package tasks first")
    image = ROOT / "target/release-images" / TARGET / "image/manifest.json"
    if not image.is_file():
        raise SystemExit("Missing native image; run deploy:image-build first")
    with tarfile.open(directory / "lorebay-deploy.tar.gz", "w:gz") as archive:
        archive.add(ROOT / "deploy", arcname="deploy", filter=metadata)
    checksums = []
    for path in sorted(directory.glob("*.tar.gz")):
        with path.open("rb") as source:
            checksums.append(f"{hashlib.file_digest(source, 'sha256').hexdigest()}  {path.name}\n")
    (directory / "SHA256SUMS").write_text("".join(checksums))
    print(f"Prepared native deployment bundle and SHA256SUMS: {directory}")


if __name__ == "__main__":
    main()
