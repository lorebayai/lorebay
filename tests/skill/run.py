import json
import os
import signal
import sys
import tempfile
import uuid
from pathlib import Path

from configuration import RunInterrupted, load_configuration, load_scenarios
from harness import Harness, sanitize, scrub


sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "common"))
from callers import home_leases, prepare_callers
from checks import Checks, expand, server_tools
from leases import HomeInUse


def write(path, value, clean):
    content = json.dumps(scrub(value, clean), indent=2) + "\n"
    with open(path, "x", opener=lambda name, flags: os.open(name, flags, 0o600)) as output:
        output.write(content)


def attempt(config, scenario, selected, directory, evidence, callers_for_attempt):
    callers = callers_for_attempt(config, directory / "clients")
    clients, agents = callers.clients, callers.agents
    people = [config["people"][name] for name in ("owner", "other", "admin")]
    def clean(text):
        return sanitize(text, clients.secrets)

    sessions, result, checks = {}, {"expectations": [], "sessions": {}, "error": None}, None
    try:
        space = clients.cli("space", "create", "--name", "Skill acceptance " + uuid.uuid4().hex[:12],
                            "--summary", scenario["name"])["key"]
        clients.cli("member", "add", space, people[1]["username"], "--role", "contributor")
        clients.cli("member", "add", space, people[2]["username"], "--role", "contributor")
        clients.cli("member", "set-role", space, people[2]["username"], "--role", "admin")
        clients.cli("space", "show", space, person=2)
        project = directory / "project"
        project.mkdir(mode=0o700)
        (project / "AGENTS.md").write_text(f'- Lorebay space: `{space}`\n')
        (project / "CLAUDE.md").symlink_to("AGENTS.md")
        same_home = "codex" if selected == "claude" else "claude"
        variables = {"space": space, "owner": people[0]["username"], "other": people[1]["username"],
                     "primary_agent": config["homes"][selected]["agent"],
                     "same_agent": config["homes"][same_home]["agent"],
                     "other_agent": config["homes"]["other"]["agent"]}
        checks = Checks(clients, space, variables, agents, project)
        expected_tools = server_tools(agents["owner"])
        for action in scenario["starting_state"]:
            if isinstance(action, dict):
                checks.action(action)
        for step in scenario["steps"]:
            if "action" in step:
                checks.action(step["action"])
            if "turn" in step:
                role = step.get("role", "primary")
                home_name = selected if role == "primary" else (
                    ("codex" if selected == "claude" else "claude") if role == "same_person" else "other")
                if role not in sessions or step.get("new_session"):
                    key = role + "-" + str(len(sessions))
                    sessions[key] = Harness(config["homes"][home_name], project, config.get("timeout", 600), clean,
                                            callers.locks.get(config["homes"][home_name]["lock"]))
                    sessions[role] = sessions[key]
                sessions[role].turn(expand(step["turn"], variables))
            for expectation in step.get("expect", []):
                checks.check(expectation)
    except RunInterrupted:
        result["error"] = "Run interrupted"
        raise
    except Exception as error:
        result["error"] = clean(str(error))
        print("FAIL scenario: " + result["error"], flush=True)
    finally:
        for name, session in sessions.items():
            if "-" in name:
                if checks and result["error"] != "Run interrupted":
                    try:
                        if session.home["harness"] == "codex":
                            session.codex_tools()
                        tools_pass = session.tools == expected_tools and bool(expected_tools)
                    except RunInterrupted:
                        result["error"] = "Run interrupted"
                        tools_pass = False
                    except Exception:
                        tools_pass = False
                    for label, passed in (("tools " + name, tools_pass), ("loaded SKILL.md " + name, session.loaded)):
                        checks.results.append({"label": label, "passed": bool(passed)})
                        print(f'{"PASS" if passed else "FAIL"} {label}', flush=True)
                    print(f'SKILL.md {session.skill} sha256={session.digest} loaded={session.loaded}', flush=True)
                result["sessions"][name] = session.record()
        if checks:
            result["expectations"] = checks.results
        clients.close()
        result["passed"] = not result["error"] and bool(result["expectations"]) and all(
            row["passed"] for row in result["expectations"])
        write(evidence / "transcript.json", result, clean)
    if result["error"] == "Run interrupted":
        raise RunInterrupted("Run interrupted")
    return result


def run(config):
    with home_leases(config) as locks:
        def configured_callers(options, directory):
            return prepare_callers(options, directory, locks)

        return execute(config, configured_callers)


def execute(config, callers_for_attempt):
    root = Path(config["output"]) / ("run-" + uuid.uuid4().hex[:12])
    root.mkdir(mode=0o700, parents=True)
    report = {"scenarios": []}
    questions = []
    def clean(text):
        return sanitize(text, ())

    for scenario in load_scenarios():
        if scenario["number"] not in config["scenarios"]:
            continue
        for selected in config["harnesses"]:
            if selected not in scenario["harnesses"]:
                continue
            identifier = f'{scenario["number"]:02d}-{selected}'
            attempts = []
            for number in (1, 2):
                directory = root / f"{identifier}-attempt-{number}"
                directory.mkdir(mode=0o700)
                with tempfile.TemporaryDirectory(prefix="runtime-", dir=directory) as temporary:
                    runtime = Path(temporary)
                    result = attempt(config, scenario, selected, runtime, directory, callers_for_attempt)
                attempts.append({"passed": result["passed"], "transcript": str(directory / "transcript.json")})
                if result["passed"]:
                    break
            status = ("pass on rerun" if len(attempts) == 2 else "pass") if attempts[-1]["passed"] else "fail"
            report["scenarios"].append({"id": identifier, "automation": status, "attempts": attempts})
            questions.append({"id": identifier, "questions": scenario["questions"],
                              "answers": [None] * len(scenario["questions"])})
            print(f"{identifier}: {status}; person's verdict pending", flush=True)
    write(root / "results.json", report, clean)
    write(root / "verdict.json", {"scenarios": questions}, clean)
    print(f"Evidence and verdict: {root}", flush=True)
    return 1 if any(row["automation"] == "fail" for row in report["scenarios"]) else 2


def verdict(directory):
    report = json.loads((directory / "results.json").read_text())
    responses = json.loads((directory / "verdict.json").read_text())["scenarios"]
    by_id = {row["id"]: row for row in responses}
    expected_questions = {f'{scenario["number"]:02d}-{harness}': scenario["questions"]
                          for scenario in load_scenarios() for harness in scenario["harnesses"]}
    passed = bool(report["scenarios"]) and len(by_id) == len(responses) == len(report["scenarios"])
    for row in report["scenarios"]:
        response = by_id.get(row["id"], {})
        questions = response.get("questions", [])
        answers = response.get("answers", [])
        confirmed = (questions == expected_questions.get(row["id"]) and bool(questions)
                     and len(answers) == len(questions) and all(answer is True for answer in answers))
        accepted = row["automation"] in {"pass", "pass on rerun"} and confirmed
        print(f'{"PASS" if accepted else "FAIL"} {row["id"]}: {row["automation"]}; person={confirmed}')
        passed = passed and accepted
    return 0 if passed else 1


def main():
    def interrupt(_signum, _frame):
        raise RunInterrupted("Run interrupted")

    for signum in (signal.SIGINT, signal.SIGTERM):
        signal.signal(signum, interrupt)
    if len(sys.argv) != 2:
        print("Usage: python3 -B tests/skill/run.py CONFIG.json")
        return 1
    try:
        config = load_configuration(sys.argv[1])
        return run(config)
    except HomeInUse as error:
        print("FAIL " + str(error))
        return 1
    except Exception:
        print("FAIL configuration or run interrupted; no raw diagnostic retained")
        return 1


if __name__ == "__main__":
    sys.exit(main())
