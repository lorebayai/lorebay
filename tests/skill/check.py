import json
from pathlib import Path

from configuration import load_scenarios, validate_configuration


directory = Path(__file__).parent
for path in [*directory.glob("*.py"), *directory.parent.glob("common/*.py")]:
    source = path.read_text()
    compile(source, str(path), "exec")
    for number, line in enumerate(source.splitlines(), 1):
        if len(line) > 120:
            raise SystemExit(f"{path}:{number}: line exceeds 120 characters")
scenarios = load_scenarios()
example = json.loads((directory / "example.json").read_text())
validate_configuration(example)
if example["scenarios"] != [row["number"] for row in scenarios]:
    raise SystemExit("Example must select every scenario")
if not (directory / "CLAUDE.md").is_symlink():
    raise SystemExit("CLAUDE.md must link to AGENTS.md")
print("Skill suite syntax, configuration example and 13 scenarios passed")
