import json
from pathlib import Path


directory = Path(__file__).parent
paths = [*directory.glob("*.py"), *directory.glob("Dockerfile.*"), *directory.parent.glob("common/*.py")]
for path in [*paths, directory.parent / "e2e/journeys.py"]:
    source = path.read_text()
    if path.suffix == ".py":
        compile(source, str(path), "exec")
    for number, line in enumerate(source.splitlines(), 1):
        if len(line) > 120:
            raise SystemExit(f"{path}:{number}: line exceeds 120 characters")
json.loads((directory / "example.json").read_text())
print("Deployment suite syntax and example JSON passed")
