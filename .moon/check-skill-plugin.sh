#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

python3 - <<'PY'
import json
from pathlib import Path
import sys

errors = []


def load(path):
    try:
        data = json.loads(Path(path).read_text())
    except (OSError, ValueError) as error:
        errors.append(f"{path}: {error}")
        return {}
    if isinstance(data, dict):
        return data
    errors.append(f"{path}: is not a JSON object")
    return {}


marketplaces = {
    ".claude-plugin/marketplace.json": "./skill",
    ".agents/plugins/marketplace.json": {"source": "local", "path": "./skill"},
}
for path, source in marketplaces.items():
    marketplace = load(path)
    if marketplace.get("name") != "lorebay":
        errors.append(f"{path}: the marketplace is not named lorebay")
    plugins = marketplace.get("plugins", [])
    if [(p.get("name"), p.get("source")) for p in plugins] != [("lorebay", source)]:
        errors.append(f"{path}: the plugins are not exactly lorebay at ./skill")
    for entry in [marketplace, *plugins]:
        for key in ("version", "mcpServers"):
            if key in entry:
                errors.append(f"{path}: has {key}")

manifests = ["skill/.claude-plugin/plugin.json", "skill/.codex-plugin/plugin.json"]
claude, codex = (load(path) for path in manifests)
if claude != codex:
    errors.append(f"{manifests[0]} and {manifests[1]} differ")
fields = ("name", "description", "author", "homepage", "repository", "license", "skills")
missing = [field for field in fields if field not in claude]
if missing:
    errors.append(f"{manifests[0]}: lacks {', '.join(missing)}")
if claude.get("name") != "lorebay" or claude.get("skills") != ["./lorebay"]:
    errors.append(f"{manifests[0]}: is not the plugin lorebay with the skill ./lorebay")
for key in ("version", "mcpServers"):
    if key in claude:
        errors.append(f"{manifests[0]}: has {key}")
if Path("skill/.mcp.json").exists():
    errors.append("skill/.mcp.json: the plugin declares an MCP server")

if errors:
    print("Skill plugin violations:", file=sys.stderr)
    for error in errors:
        print(error, file=sys.stderr)
    sys.exit(1)
PY
