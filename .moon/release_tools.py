import hashlib
import os
import platform
import shutil
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path


# Later releases omit Intel macOS binaries, which the dependency check supports.
TOOLS = {
    "cargo-deny": (
        "0.20.2",
        {
            "aarch64-apple-darwin": "fe67d82a10d8597a3549364cb733a3f9cc1bfff9031b7ae46384a9f2a72090c3",
            "aarch64-unknown-linux-musl": "995c82be0defc7a025cae49a2aa2644ce8245c9a3318fc4103907c6a285e8c7d",
            "x86_64-apple-darwin": "248da7f581724e470071990c088ffc55c811981715f4cbdb258621fb79f8b7a6",
            "x86_64-unknown-linux-musl": "9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f",
        },
    ),
    "cargo-about": (
        "0.8.4",
        {
            "aarch64-apple-darwin": "d5255ead3ac861a11c785bf19d4f70f16e59ac8b9519c312da61213d3d57351d",
            "aarch64-unknown-linux-musl": "33e0d54c0366308095fdb9101642d281b224131299e1c62001f43aa02bb3573e",
            "x86_64-apple-darwin": "e918d49c481f43eb17da92c406839d5c01d480261d8026a017f3d23706009d0d",
            "x86_64-unknown-linux-musl": "c7381aa0cdc41fc0ee662cec8daa260da7817ad8ddea04cd4ddad425460adf14",
        },
    ),
}


def install_tools():
    architecture = {"arm64": "aarch64", "amd64": "x86_64"}.get(
        platform.machine().lower(), platform.machine().lower()
    )
    system = {"Darwin": "apple-darwin", "Linux": "unknown-linux-musl"}.get(
        platform.system()
    )
    target = f"{architecture}-{system}"
    if target not in TOOLS["cargo-deny"][1]:
        raise SystemExit(
            "Supported platforms: Linux x86_64 and aarch64; macOS x86_64 and aarch64."
        )
    cache = Path(__file__).resolve().parent.parent / "target" / "release-tools"
    cache.mkdir(parents=True, exist_ok=True)
    installed = {}
    for name, (version, checksums) in TOOLS.items():
        stem = f"{name}-{version}-{target}"
        archive = cache / f"{stem}.tar.gz"
        with tempfile.TemporaryDirectory(dir=cache) as temporary:
            temporary = Path(temporary)
            if not archive.exists():
                url = f"https://github.com/EmbarkStudios/{name}/releases/download/{version}/{archive.name}"
                print(f"Downloading {archive.name}", flush=True)
                with urllib.request.urlopen(url, timeout=120) as response:
                    with (temporary / archive.name).open("wb") as output:
                        shutil.copyfileobj(response, output)
                candidate = temporary / archive.name
            else:
                candidate = archive
            actual = hashlib.sha256(candidate.read_bytes()).hexdigest()
            if actual != checksums[target]:
                raise SystemExit(f"SHA-256 mismatch for {archive.name}: {actual}")
            print(f"Verified SHA-256: {archive.name}", flush=True)
            if candidate != archive:
                os.replace(candidate, archive)
            with tarfile.open(archive) as contents:
                member = next(m for m in contents if Path(m.name).name == name and m.isfile())
                with contents.extractfile(member) as source:
                    with (temporary / name).open("wb") as output:
                        shutil.copyfileobj(source, output)
            binary = cache / stem
            (temporary / name).chmod(0o755)
            os.replace(temporary / name, binary)
            installed[name] = str(binary)
    return installed


if __name__ == "__main__":
    binaries = install_tools()
    os.execv(binaries[sys.argv[1]], [sys.argv[1], *sys.argv[2:]])
