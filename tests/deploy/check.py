import json
from pathlib import Path


directory = Path(__file__).parent
for path in directory.glob("*.py"):
    compile(path.read_text(), str(path), "exec")
json.loads((directory / "example.json").read_text())
print("Deployment suite syntax and example JSON passed")
