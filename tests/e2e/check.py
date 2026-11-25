from pathlib import Path


directory = Path(__file__).parent
for path in [*directory.glob("*.py"), *directory.parent.glob("common/*.py")]:
    source = path.read_text()
    compile(source, str(path), "exec")
    for number, line in enumerate(source.splitlines(), 1):
        if len(line) > 120:
            raise SystemExit(f"{path}:{number}: line exceeds 120 characters")
print("E2E suite and shared client syntax passed")
