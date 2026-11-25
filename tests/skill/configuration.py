import json
import urllib.parse
from pathlib import Path


DIRECTORY = Path(__file__).resolve().parent
WORKSPACE = DIRECTORY.parents[1]
OPERATORS = {"equals", "contains", "at_least", "nonempty", "count", "absent", "matches"}


class RunInterrupted(Exception):
    pass


def require(condition, message):
    if not condition:
        raise ValueError(message)


def outside(path):
    require(Path(path).expanduser().is_absolute(), "Runtime paths must be absolute")
    result = Path(path).expanduser().resolve()
    require(not result.is_relative_to(WORKSPACE), "Runtime paths must be outside the checkout")
    return result


def validate_action(action):
    require(isinstance(action, dict), "Actions must be objects")
    require(sum(key in action for key in ("cli", "tool", "query")) == 1, "An action needs exactly one caller")
    require(set(action) <= {"cli", "tool", "query", "person", "agent", "arguments", "save", "failure"},
            "Unknown action field")
    if "query" in action:
        validate_query(action["query"])


def validate_query(query):
    require(isinstance(query, dict), "Queries must be objects")
    require(("cli" in query) != ("variable" in query), "A query needs exactly one source")
    require(set(query) <= {"cli", "person", "select", "path", "variable", "download"}, "Unknown query field")


def load_scenarios():
    scenarios = [json.loads(path.read_text()) for path in sorted((DIRECTORY / "scenarios").glob("*.json"))]
    require([row["number"] for row in scenarios] == list(range(1, 14)), "Expected scenarios 1 through 13")
    for row in scenarios:
        require(set(row) == {"number", "name", "harnesses", "roles", "starting_state", "steps", "questions"},
                "Invalid scenario fields")
        require(row["harnesses"] and set(row["harnesses"]) <= {"claude", "codex"}, "Invalid harnesses")
        require(row["roles"] and row["questions"] and row["starting_state"], "Scenario is incomplete")
        require(any("turn" in step for step in row["steps"]), "Scenario needs a person's turn")
        require(any(step.get("expect") for step in row["steps"]), "Scenario needs end-state expectations")
        for action in row["starting_state"]:
            if isinstance(action, dict):
                validate_action(action)
        for step in row["steps"]:
            require(set(step) <= {"turn", "role", "new_session", "action", "expect"}, "Unknown step field")
            if "turn" in step:
                require(step.get("role", "primary") in row["roles"], "Unknown turn role")
            if "action" in step:
                validate_action(step["action"])
            for expectation in step.get("expect", []):
                require(expectation["op"] in OPERATORS, "Unknown expectation operator")
                require("label" in expectation and "query" in expectation, "Expectation needs a label and query")
                validate_query(expectation["query"])
                if expectation["op"] in {"contains", "absent", "matches"}:
                    require(isinstance(expectation.get("value"), str), "Text expectations need a string")
                if expectation["op"] in {"count", "at_least"}:
                    require(isinstance(expectation.get("value"), int) and expectation["value"] >= 0,
                            "Count expectations need a nonnegative integer")
    return scenarios


def load_configuration(path):
    path = outside(path)
    config = json.loads(path.read_text())
    return validate_configuration(config)


def validate_configuration(config):
    required = {"origin", "authority", "people", "script_agents", "homes", "cli", "output", "scenarios", "harnesses"}
    require(required <= set(config), "Missing configuration keys")
    require(set(config) <= required | {"timeout"}, "Unknown configuration key")
    origin = urllib.parse.urlsplit(config["origin"])
    require(origin.scheme in {"http", "https"} and origin.netloc and not origin.username and not origin.password
            and origin.path in {"", "/"} and not origin.query and not origin.fragment, "Invalid origin")
    require(config["authority"] or origin.scheme == "http", "HTTPS requires an authority")
    config["output"] = str(outside(config["output"]))
    require(config["scenarios"] and set(config["scenarios"]) <= set(range(1, 14)), "Invalid scenario selection")
    require(config["harnesses"] and set(config["harnesses"]) <= {"claude", "codex"}, "Invalid harness selection")
    require(set(config["people"]) == {"owner", "other", "admin"}, "Configure three CLI people")
    for person in config["people"].values():
        require(set(person) == {"username", "home", "lock"} and person["username"], "Invalid CLI profile")
        person["home"] = str(outside(person["home"]))
    require(len({person["username"] for person in config["people"].values()}) == 3, "CLI people must differ")
    require(set(config["script_agents"]) == {"owner", "other"}, "Configure two script agents")
    for agent in config["script_agents"].values():
        require(set(agent) == {"command", "home", "lock"} and agent["command"], "Invalid script-agent command")
        agent["home"] = str(outside(agent["home"]))
    require(set(config["homes"]) == {"claude", "codex", "other"}, "Configure three harness homes")
    for name, home in config["homes"].items():
        require(set(home) == {"harness", "person", "agent", "command", "environment", "skill", "lock"},
                "Invalid home keys")
        require(home["harness"] == ("codex" if name == "other" else name), "Unexpected home harness")
        require(home["person"] == ("other" if name == "other" else "owner"), "Unexpected home person")
        require(home["command"] and all(isinstance(part, str) for part in home["command"]), "Invalid command")
        require(Path(home["skill"]).name == "SKILL.md", "Only the installed SKILL.md may be read")
        home["skill"] = str(outside(home["skill"]))
        require(Path(home["skill"]).name == "SKILL.md", "Installed skill must resolve to SKILL.md")
        require("HOME" in home["environment"], "Each harness needs its own HOME")
        outside(home["environment"]["HOME"])
        require(set(home["environment"]) <= {
            "HOME", "CLAUDE_CONFIG_DIR", "CODEX_HOME", "PATH", "NODE_EXTRA_CA_CERTS", "CODEX_CA_CERTIFICATE",
            "SSL_CERT_FILE", "CURL_CA_BUNDLE", "BROWSER",
        }, "Home environment contains an unsupported key")
    require(len({home["environment"]["HOME"] for home in config["homes"].values()}) == 3,
            "Harness homes must differ")
    entries = [entry for section in ("people", "script_agents", "homes") for entry in config[section].values()]
    paths = []
    for entry in entries:
        entry["lock"] = str(outside(entry["lock"]))
        profile = outside(entry.get("home") or entry["environment"]["HOME"])
        require(Path(entry["lock"]) == profile.parent / ".locks" / (profile.name + ".lock"),
                "Home lock must use the profile's shared lock path")
        paths.append(profile)
    require(len(set(paths)) == len(paths), "Each role requires a distinct home")
    require(len({entry["lock"] for entry in entries}) == len(entries), "Each role requires a distinct home lock")
    return config
