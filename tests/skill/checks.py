import hashlib
import json
import re
from typing import Any

from configuration import require


def expand(value, variables) -> Any:
    if isinstance(value, str):
        for name, replacement in variables.items():
            marker = "{" + name + "}"
            if value == marker:
                return replacement
            value = value.replace(marker, str(replacement))
    elif isinstance(value, list):
        return [expand(item, variables) for item in value]
    elif isinstance(value, dict):
        return {key: expand(item, variables) for key, item in value.items()}
    return value


class Checks:
    def __init__(self, clients, space, variables, agents, project):
        self.clients, self.space, self.variables, self.agents = clients, space, variables, agents
        self.results = []
        self.project = project

    def action(self, action):
        action = expand(action, self.variables)
        if "query" in action:
            value = self.query(action["query"])
        elif "cli" in action:
            value = self.clients.cli(*action["cli"], person=action.get("person", 0), failure=action.get("failure"))
        else:
            value = self.agents[action.get("agent", "owner")].call(action["tool"], **action.get("arguments", {}))
        for name, path in action.get("save", {}).items():
            item = value
            for part in path:
                item = item[part]
            self.variables[name] = item
        return value

    def query(self, query):
        query = expand(query, self.variables)
        if "variable" in query:
            value = self.variables[query["variable"]]
        else:
            value = self.clients.cli(*query["cli"], person=query.get("person", 0))
        if "select" in query:
            selected = [row for row in value if all(row.get(key) == item for key, item in query["select"].items())]
            require(len(selected) == 1, "Expected exactly one matching record")
            value = selected[0]
        for part in query.get("path", []):
            value = value[part]
        if query.get("download"):
            require(len(value) > 0, "Expected at least one uploaded file")
            for file in value:
                url = self.clients.cli("file", "url", self.space, file["key"])["url"]
                size, digest = self.clients.download(url)
                require(size == file["size"] and len(digest) == hashlib.sha256().digest_size * 2,
                        "Uploaded file is incomplete or unreadable")
                local = (self.project / file["file_name"]).resolve()
                require(local.is_relative_to(self.project.resolve()) and local.is_file(),
                        "Uploaded deliverable is missing from the project")
                with local.open("rb") as source:
                    require(digest == hashlib.file_digest(source, "sha256").hexdigest(),
                            "Uploaded deliverable differs from the final local file")
        return value

    def check(self, expectation):
        label = expectation["label"]
        try:
            actual = self.query(expectation["query"])
            expected = expand(expectation.get("value"), self.variables)
            operation = expectation["op"]
            if operation == "equals":
                passed = actual == expected
            elif operation == "contains":
                passed = expected in (actual if isinstance(actual, str) else json.dumps(actual))
            elif operation == "at_least":
                passed = len(actual) >= expected
            elif operation == "count":
                passed = len(actual) == expected
            elif operation == "absent":
                passed = expected not in json.dumps(actual)
            elif operation == "matches":
                passed = isinstance(actual, str) and bool(re.search(expected, actual, re.IGNORECASE))
            else:
                passed = bool(actual)
            require(passed, "Expectation differs from the state read back")
            self.results.append({"label": label, "passed": True})
        except (ValueError, RuntimeError, KeyError, IndexError) as error:
            self.results.append({"label": label, "passed": False, "reason": self.clients.redact(str(error))})
        print(f'{"PASS" if self.results[-1]["passed"] else "FAIL"} {label}', flush=True)


def server_tools(clients, agent):
    tools, cursor = set(), None
    while True:
        response = clients.api("POST", "/mcp", {
            "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {"cursor": cursor} if cursor else {},
        }, agent.token, headers={"Accept": "application/json, text/event-stream"})
        require("error" not in response, "Server tool discovery failed")
        tools.update(tool["name"] for tool in response["result"]["tools"])
        cursor = response["result"].get("nextCursor")
        if not cursor:
            return tools
