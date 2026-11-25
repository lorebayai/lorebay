import sys
from pathlib import Path

from run import verdict


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python3 -B tests/skill/verdict.py /path/to/run-directory")
    sys.exit(verdict(Path(sys.argv[1])))
