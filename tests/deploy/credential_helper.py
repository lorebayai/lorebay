#!/usr/bin/env python3
import json
import os
import sys


if sys.argv[1] != "get":
    raise SystemExit(1)
sys.stdin.read()
print(json.dumps({"Username": os.environ["DEPLOY_GITHUB_ACCOUNT"], "Secret": os.environ["GH_TOKEN"]}))
