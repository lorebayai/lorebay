#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

python3 - <<'PY'
import json
from html.parser import HTMLParser
from os.path import relpath
from pathlib import Path
import re
import subprocess
import sys
import tomllib
from urllib.parse import unquote, urlsplit

root = Path.cwd().resolve()
library = root / "lib"
playbook = root / "playbook"
errors = []


def fail_if_errors():
    if errors:
        print("Library boundary violations:", file=sys.stderr)
        for error in errors:
            print(error, file=sys.stderr)
        sys.exit(1)


# Cargo reads config from the invocation directory and its ancestors.
# A downstream git dependency uses the consumer's config and patches.
with (root / "Cargo.toml").open("rb") as manifest:
    if "patch" in tomllib.load(manifest):
        errors.append("Cargo.toml: [patch] does not apply to a product that depends on the library")
for config_dir in (root / ".cargo", *sorted(library.rglob(".cargo"))):
    for filename in ("config.toml", "config"):
        config = config_dir / filename
        if config.exists():
            errors.append(f"{config.relative_to(root)}: Cargo config does not apply to a product that depends on the library")
fail_if_errors()


def check_path(source, target, kind, allow_playbook=False):
    resolved = target.resolve()
    if resolved.is_relative_to(library):
        return
    if allow_playbook and resolved.is_relative_to(playbook):
        return
    errors.append(f"{source.relative_to(root)}: {kind} leaves lib/: {relpath(resolved, root)}")


def check_link(source, destination):
    url = urlsplit(unquote(destination.strip("<>")))
    if not url.path:
        return
    if url.scheme or url.netloc:
        prefix = "/lorebayai/lorebay/"
        if url.netloc == "github.com" and url.path.startswith(prefix):
            parts = url.path[len(prefix):].split("/", 2)
            if len(parts) == 3 and parts[0] in ("blob", "tree"):
                check_path(source, root / parts[2], "link", source.suffix == ".md")
        return
    target = root / url.path.lstrip("/") if url.path.startswith("/") else source.parent / url.path
    check_path(source, target, "link", source.suffix == ".md")


class HtmlLinks(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.source = source

    def handle_starttag(self, _tag, attrs):
        for name, value in attrs:
            if name in ("href", "src", "xlink:href") and value is not None:
                check_link(self.source, value)


files = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "lib/"],
    text=True,
).split("\0")
link_patterns = [
    r'\[[^\]\n]*\]\(\s*(<[^>]+>|[^\s)]+)',
    r'^\s*(?:///\s*)?\[[^\]\n]+\]:\s*(<[^>]+>|\S+)',
    r'\b(?:href|src)\s*=\s*[\"\x27]([^\"\x27]+)[\"\x27]',
    r'<((?:https?://|/|\.\.?/)[^>\s]+)>',
]
for name in sorted(set(filter(None, files))):
    source = root / name
    if source.is_symlink():
        check_path(source, source, "symlink")
        continue
    if not source.is_file():
        continue
    try:
        content = source.read_text()
    except UnicodeDecodeError:
        continue
    if source.suffix.lower() in (".htm", ".html"):
        HtmlLinks(source).feed(content)
    else:
        for pattern in link_patterns:
            for match in re.finditer(pattern, content, re.MULTILINE):
                check_link(source, match[1])
    if source.name == "AGENTS.md":
        for match in re.finditer(r'^\s*(?:-\s+)?@([^\s]+)', content, re.MULTILINE):
            check_path(source, source.parent / match[1], "import", True)
    if source.suffix == ".rs":
        for match in re.finditer(r'(?:include(?:_str|_bytes)?!\s*\(|#\[path\s*=)\s*"([^"]+)"', content):
            check_path(source, source.parent / match[1], "source include")

metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
    text=True,
))
manifests = {Path(package["manifest_path"]).resolve() for package in metadata["packages"]}
product_crates = {
    package["name"] for package in metadata["packages"]
    if not Path(package["manifest_path"]).resolve().is_relative_to(library)
}
for name in filter(None, files):
    source = root / name
    if source.name == "Cargo.toml" and source.resolve() not in manifests:
        errors.append(f"{name}: library crate is missing from the Cargo workspace")
for package in metadata["packages"]:
    manifest = Path(package["manifest_path"])
    if not manifest.resolve().is_relative_to(library):
        continue
    for target in package["targets"]:
        check_path(manifest, Path(target["src_path"]), "Cargo target")
    for dependency in package["dependencies"]:
        if "path" in dependency:
            check_path(manifest, Path(dependency["path"]), "dependency")
        elif dependency["name"] in product_crates:
            errors.append(f"{manifest.relative_to(root)}: dependency on product crate {dependency['name']}")

library_versions = {
    package["name"]: package["version"] for package in metadata["packages"]
    if Path(package["manifest_path"]).resolve().is_relative_to(library)
}
if len(set(library_versions.values())) > 1:
    listed = ", ".join(f"{name} {version}" for name, version in sorted(library_versions.items()))
    errors.append(f"library crates must share one version: {listed}")

fail_if_errors()
print("Library boundary check passed.")
PY
