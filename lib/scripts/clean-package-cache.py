import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess


def registry_hash(registry):
    # Cargo hashes LocalRegistry's discriminant and canonical URL with stable SipHash-1-3.
    # https://doc.rust-lang.org/nightly/nightly-rustc/src/cargo/sources/registry/mod.rs.html
    data = b"\x04" + registry.as_uri().encode() + b"\xff"
    mask = (1 << 64) - 1
    state = [
        0x736F6D6570736575, 0x646F72616E646F6D ^ 0xEE,
        0x6C7967656E657261, 0x7465646279746573,
    ]

    def rotate(value, bits):
        return ((value << bits) | (value >> (64 - bits))) & mask

    def rounds(count):
        for _ in range(count):
            a, b, c, d = state
            a = (a + b) & mask
            b = rotate(b, 13) ^ a
            a = rotate(a, 32)
            c = (c + d) & mask
            d = rotate(d, 16) ^ c
            a = (a + d) & mask
            d = rotate(d, 21) ^ a
            c = (c + b) & mask
            b = rotate(b, 17) ^ c
            c = rotate(c, 32)
            state[:] = a, b, c, d

    end = len(data) // 8 * 8
    blocks = [int.from_bytes(data[i:i + 8], "little") for i in range(0, end, 8)]
    blocks.append(((len(data) & 255) << 56) | int.from_bytes(data[end:], "little"))
    for block in blocks:
        state[3] ^= block
        rounds(1)
        state[0] ^= block
    state[2] ^= 0xEE
    rounds(3)
    first = state[0] ^ state[1] ^ state[2] ^ state[3]
    state[1] ^= 0xDD
    rounds(3)
    second = state[0] ^ state[1] ^ state[2] ^ state[3]
    return ((first * 3 + second) & mask).to_bytes(8, "little").hex()


def remove(path):
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    else:
        path.unlink(missing_ok=True)


def clean_builds(build, sources):
    units = set()
    files = set()
    for dep_info in build.glob("**/lorebay_lib_*.d"):
        _, separator, dependencies = dep_info.read_text().partition(": ")
        if not separator:
            continue
        dependencies = shlex.split(dependencies.splitlines()[0])
        if not any(Path(source).is_relative_to(sources) for source in dependencies):
            continue
        if (dep_info.parent.name == "out"
                and dep_info.parent.parent.parent.name.startswith("lorebay-lib-")):
            units.add(dep_info.parent.parent)
        elif dep_info.parent.name == "deps":
            name, unit_hash = dep_info.stem.rsplit("-", 1)
            units.add(dep_info.parent.parent / ".fingerprint"
                      / f"{name.replace('_', '-')}-{unit_hash}")
            files.add(dep_info)
            files.update(dep_info.parent.glob(f"lib{dep_info.stem}.*"))
    for path in sorted(units | files):
        remove(path)
    return len(units)


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
        text=True,
    ))
    build = Path(metadata.get("build_directory", metadata["target_directory"])).resolve()
    registry = build / "package" / "tmp-registry"
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    sources = cargo_home / "registry" / "src" / f"-{registry_hash(registry)}"
    units = clean_builds(build, sources)
    packages = list(sources.glob("lorebay-lib-*"))
    for package in packages:
        remove(package)
    print(f"Cleared {units} temporary-registry library builds and {len(packages)} library sources.")


if __name__ == "__main__":
    main()
