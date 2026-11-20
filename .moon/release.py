import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

from release_tools import install_tools


ROOT = Path(__file__).resolve().parent.parent
TARGETS = tomllib.loads((ROOT / "deny.toml").read_text())["graph"]["targets"]
EXECUTABLES = {"cli": "lorebay", "lorebayd": "lorebayd"}


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def version():
    tag = os.environ["GITHUB_REF_NAME"]
    number = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?", tag):
        raise SystemExit(f"Invalid release tag: {tag}")
    if tag != f"v{number}":
        raise SystemExit(f"Tag {tag} does not match workspace version {number}")
    print(f"Release version: {number}")
    return number


def deny(project):
    tools = install_tools()
    manifest = ROOT / project / "Cargo.toml"
    metadata = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1", "--manifest-path", str(manifest), capture_output=True).stdout)
    # cargo-deny lacks a build-dependency filter; notices and checks share the normal graph.
    for node in metadata["resolve"]["nodes"]:
        node["deps"] = [d for d in node["deps"] if any(k["kind"] is None for k in d["dep_kinds"])]
        for dependency in node["deps"]:
            dependency["dep_kinds"] = [k for k in dependency["dep_kinds"] if k["kind"] is None]
        node["dependencies"] = [d["pkg"] for d in node["deps"]]
    for package in metadata["packages"]:
        package["dependencies"] = [d for d in package["dependencies"] if d["kind"] is None]
    directory = ROOT / "target" / "release-metadata" / project
    directory.mkdir(parents=True, exist_ok=True)
    graph = directory / "normal.json"
    graph.write_text(json.dumps(metadata))
    run(tools["cargo-deny"], "--manifest-path", str(manifest), "--metadata-path", str(graph), "--config", str(ROOT / "deny.toml"), "--exclude-dev", "--locked", "check", "all")


def build(project):
    target = os.environ["RELEASE_TARGET"]
    if target not in TARGETS or not target.startswith(platform.machine().replace("arm64", "aarch64")):
        raise SystemExit(f"Release builds need a native Linux runner for {target}")
    run("rustup", "target", "add", target)
    environment = dict(os.environ)
    environment[f"CARGO_TARGET_{target.upper().replace('-', '_')}_LINKER"] = "musl-gcc"
    environment[f"CC_{target.replace('-', '_')}"] = "musl-gcc"
    run("cargo", "build", "--release", "--locked", "--manifest-path", str(ROOT / project / "Cargo.toml"), "--bin", EXECUTABLES[project], "--target", target, env=environment)


def setup():
    run("sudo", "apt-get", "update")
    run("sudo", "apt-get", "install", "-y", "musl-tools", "cmake", "pkg-config")


def notices(project):
    target = os.environ["RELEASE_TARGET"]
    if target not in TARGETS:
        raise SystemExit(f"Unsupported release target: {target}")
    executable = EXECUTABLES[project]
    tools = install_tools()
    directory = ROOT / "target" / "release-archives" / target / executable
    directory.mkdir(parents=True, exist_ok=True)
    allowed = tomllib.loads((ROOT / "deny.toml").read_text())["licenses"]["allow"]
    config = directory / "about.toml"
    config.write_text(f"accepted = {json.dumps(allowed)}\nignore-build-dependencies = true\nignore-dev-dependencies = true\nno-clearly-defined = true\n")
    template = directory / "notices.hbs"
    template.write_text("# Third-party notices\n\n{{#each licenses}}\n## {{{name}}}\n\n{{#each used_by}}- {{crate.name}} {{crate.version}}\n{{/each}}\n{{{text}}}\n\n{{/each}}")
    notices = directory / "THIRD-PARTY-NOTICES.txt"
    run(tools["cargo-about"], "generate", "--locked", "--fail", "--manifest-path", str(ROOT / project / "Cargo.toml"), "--target", target, "--config", str(config), "--output-file", str(notices), str(template))
    return directory


def package(project):
    number = version()
    target = os.environ["RELEASE_TARGET"]
    executable = EXECUTABLES[project]
    binary = ROOT / "target" / target / "release" / executable
    actual = run(str(binary), "--version", capture_output=True).stdout.strip()
    if actual != f"{executable} {number}":
        raise SystemExit(f"Unexpected executable version: {actual}")
    elf = run("readelf", "-l", "-d", str(binary), capture_output=True).stdout
    if "INTERP" in elf or "(NEEDED)" in elf:
        raise SystemExit(f"Executable is not static: {binary}")
    directory = notices(project)
    shutil.copy2(binary, directory / executable)
    shutil.copy2(ROOT / "LICENSE", directory / "LICENSE")
    archive = directory.parent / f"{executable}-{target}.tar.gz"
    with tarfile.open(archive, "w:gz") as output:
        for name in (executable, "LICENSE", "THIRD-PARTY-NOTICES.txt"):
            output.add(directory / name, arcname=name)
    print(f"Packaged {archive.name}: {actual}; static ELF")


def publish():
    number = version()
    directory = ROOT / "target" / "release-archives"
    expected = sorted(f"{name}-{target}.tar.gz" for name in EXECUTABLES.values() for target in TARGETS)
    archives = sorted(directory.glob("*.tar.gz"))
    if [a.name for a in archives] != expected:
        raise SystemExit(f"Expected four release archives: {expected}")
    checksums = directory / "SHA256SUMS"
    checksums.write_text("".join(f"{hashlib.sha256(a.read_bytes()).hexdigest()}  {a.name}\n" for a in archives))
    tag = f"v{number}"
    repository = os.environ["GITHUB_REPOSITORY"]
    pages = json.loads(run("gh", "api", "--paginate", "--slurp", f"repos/{repository}/releases?per_page=100", capture_output=True).stdout)
    if any(release["tag_name"] == tag for page in pages for release in page):
        raise SystemExit(f"A release already exists for {tag}; refusing to replace it")
    arguments = ["gh", "release", "create", tag, "--repo", repository, "--verify-tag", "--draft", "--title", tag, "--notes", f"Lorebay {number}. Review the archives and SHA256SUMS before publishing."]
    if "-" in number:
        arguments.append("--prerelease")
    run(*arguments, *(str(a) for a in archives), str(checksums))


if __name__ == "__main__":
    os.chdir(ROOT)
    try:
        {"version": version, "deny": deny, "setup": setup, "build": build, "notices": notices, "package": package, "publish": publish}[sys.argv[1]](*sys.argv[2:])
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
