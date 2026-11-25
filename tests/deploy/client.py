import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright


sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "common"))
from clients import Clients, require


SETTINGS = json.load(sys.stdin)
ORIGIN = SETTINGS["origin"]
PEOPLE = SETTINGS["people"]
CA = "/inputs/authority.pem"
STATE = Path("/work/state.json")
CLIENTS = Clients(ORIGIN, PEOPLE, "/work", authority=CA, chromium="/usr/bin/chromium")
CONTEXT = CLIENTS.context
api = CLIENTS.api
cli = CLIENTS.cli
agent_sign_in = CLIENTS.agent_sign_in


def login(person, device=False, wrong=False, origin=None, home=None, refused=False):
    print(
        "BEGIN cli_sign_in " + ("device" if device else "authorization") + (" refusal" if refused else ""), flush=True
    )
    return CLIENTS.login(person, device, wrong, origin, home, refused)


def upload(space, task, agent, name, payload):
    path = f"/api/v1/spaces/{space}/tasks/{task}/files"
    registered = api("POST", path, {"file_name": name, "summary": "Deployment file"}, agent, expected=201)["data"]
    CLIENTS.upload(registered, payload)
    return api("POST", path + "/" + registered["key"] + "/complete", token=agent)["data"]


def download(space, key):
    return CLIENTS.download(cli("file", "url", space, key)["url"])


def product():
    login(0, wrong=True)
    login(1, device=True)
    cli("logout")
    result = subprocess.run(
        ["lorebay", "space", "list"], env=os.environ | {"LOREBAY_HOME": "/work/person-0"}, capture_output=True
    )
    require(result.returncode == 4, "Logged-out CLI retained access")
    login(0)
    print("PASS sign_in_accepts_authorization_and_device_and_refuses_wrong_password_and_logout", flush=True)
    space = cli("space", "create", "--name", "Deployment", "--summary", "Functional check")["key"]
    require(cli("space", "show", space)["name"] == "Deployment", "Space was not created")
    cli("space", "change", space, "--name", "Deployment checked", "--summary", "Changed summary")
    shown = cli("space", "show", space)
    require(
        shown["name"] == "Deployment checked" and shown["summary"] == "Changed summary", "Space changes did not persist"
    )
    require(PEOPLE[1]["username"] in json.dumps(cli("user", "list")), "Second person is missing")
    second = PEOPLE[1]["username"]
    for role in ("viewer", "contributor", "admin"):
        cli("member", "add" if role == "viewer" else "set-role", space, second, "--role", role)
        members = cli("member", "list", space)
        require(any(m["username"] == second and m["role"] == role for m in members), "Space role did not change")
        require(cli("space", "show", space, person=1)["key"] == space, "Member cannot read space")
        create_request = ("request", "create", space, f"roles/{role}.md", "--summary", role, "--content", role)
        create_knowledge = ("knowledge", "create", space, f"roles/{role}.md", "--summary", role, "--content", role)
        if role == "viewer":
            cli(*create_request, person=1, failure={403})
        else:
            request = cli(*create_request, person=1)
            cli(
                "request", "reject", space, request["key"], "--revision", request["revision"],
                "--reason", "Role verified"
            )
        if role == "admin":
            entry = cli(*create_knowledge, person=1)
            cli("knowledge", "delete", space, entry["key"], "--version", entry["version"], "--yes")
        else:
            cli(*create_knowledge, person=1, failure={403})
    cli("member", "remove", space, second)
    require(not any(m["username"] == second for m in cli("member", "list", space)), "Removed person remains a member")
    cli("space", "show", space, person=1, failure={403, 404})
    cli(
        "request",
        "create",
        space,
        "removed.md",
        "--summary",
        "Removed",
        "--content",
        "Removed",
        person=1,
        failure={403, 404},
    )
    cli(
        "knowledge",
        "create",
        space,
        "removed.md",
        "--summary",
        "Removed",
        "--content",
        "Removed",
        person=1,
        failure={403, 404},
    )
    cli("logout", person=1)
    print("PASS spaces_preserve_changes_and_members_follow_each_role", flush=True)
    agent = agent_sign_in()
    prefix = f"/api/v1/spaces/{space}/tasks"
    task = api("POST", prefix, {"title": "Deployment work"}, agent, expected=201)["data"]
    path = prefix + "/" + task["task_id"]
    checkpoint = api(
        "POST", path + "/checkpoints", {"content": "Plan: verify deployment, then record results"}, agent, expected=201
    )["data"]
    content = b"name,result\nrelease,passed\n\x00\xff"
    small = upload(space, task["task_id"], agent, "report.csv", content)
    require(download(space, small["key"]) == (len(content), hashlib.sha256(content).hexdigest()), "File bytes changed")
    entry = cli("knowledge", "create", space, "ops/release.md", "--summary", "Release", "--content", "First revision")
    entry = cli(
        "knowledge", "change", space, entry["key"], "--version", entry["version"], "--content", "Second revision"
    )
    history = cli("knowledge", "history", space, entry["key"])
    require(len(history) == 2, "Knowledge history is incomplete")
    require(
        cli("knowledge", "show", space, entry["key"], "--version", 1)["content"] == "First revision",
        "Old Knowledge version changed",
    )
    entry = cli("knowledge", "rollback", space, entry["key"], 1, "--version", entry["version"])
    require(cli("knowledge", "show", space, entry["key"])["content"] == "First revision", "Knowledge rollback failed")
    cli("knowledge", "delete", space, entry["key"], "--version", entry["version"], "--yes")
    require(any(e["key"] == entry["key"] for e in cli("knowledge", "deleted", space)), "Deleted Knowledge is missing")
    require(len(cli("knowledge", "history", space, entry["key"])) == 4, "Rollback or deletion history is missing")
    for decision in ("approve", "reject"):
        request = api(
            "POST",
            path + "/requests",
            {
                "action": "create",
                "name": f"ops/{decision}.md",
                "summary": decision,
                "content": decision,
            },
            agent,
            expected=201,
        )["data"]
        require(any(r["key"] == request["key"] for r in cli("request", "list", space)), "Request is absent from list")
        require(cli("request", "show", space, request["key"])["status"] == "pending", "Request is not pending")
        before = cli("knowledge", "list", space)
        extra = ["--reason", "Deployment rejection"] if decision == "reject" else []
        cli("request", decision, space, request["key"], "--revision", request["revision"], *extra)
        shown = cli("request", "show", space, request["key"])
        require(
            shown["status"] == ("approved" if decision == "approve" else "rejected"), "Request decision did not persist"
        )
        if decision == "approve":
            require(
                cli("knowledge", "show", space, "ops/approve.md")["content"] == "approve",
                "Approved request did not change Knowledge",
            )
        else:
            require(
                shown["reject_reason"] == "Deployment rejection" and cli("knowledge", "list", space) == before,
                "Rejected request changed Knowledge or lost its reason",
            )
    print("PASS knowledge_preserves_history_and_rollback_and_request_decisions", flush=True)
    api("PATCH", path, {"state": "done", "result": "Deployment checked"}, agent)
    require(any(t["task_id"] == task["task_id"] for t in cli("task", "list", space)), "Task list is missing the task")
    shown = cli("task", "show", space, task["key"])
    require(shown["state"] == "done" and shown["result"] == "Deployment checked", "Task result is missing")
    require(
        cli("task", "scratchpad", space, task["key"])
        == [
            {
                "time": checkpoint["time"],
                "agent_name": "Deployment probe",
                "content": "Plan: verify deployment, then record results",
            }
        ],
        "Scratchpad differs from checkpoint",
    )
    cli("checkpoint", "delete", space, task["task_id"], checkpoint["time"], "--yes")
    require(cli("task", "scratchpad", space, task["key"]) == [], "Deleted checkpoint remains")
    cli("file", "delete", space, task["task_id"], small["key"], "--yes")
    require(cli("task", "show", space, task["key"])["files"] == [], "Deleted file remains")
    cli("file", "url", space, small["key"], failure={404})
    cli("task", "delete", space, task["task_id"], "--yes")
    require(cli("task", "list", space) == [], "Deleted task remains")
    cli("task", "show", space, task["key"], failure={404})
    STATE.write_text(json.dumps({"space": space, "agent": agent}))
    STATE.chmod(0o600)
    print("PASS tasks_preserve_results_and_files_and_delete_checkpoints_files_and_tasks", flush=True)


def large():
    state = json.loads(STATE.read_text())
    space, agent = state["space"], state["agent"]
    task = api("POST", f"/api/v1/spaces/{space}/tasks", {"title": "Large file persistence"}, agent, expected=201)[
        "data"
    ]
    size = 1073741825
    payload = Path("/work/large.bin")
    try:
        with payload.open("wb") as stream:
            stream.write(os.urandom(65536))
            stream.truncate(size)
        with payload.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        with payload.open("rb") as stream:
            file = upload(space, task["task_id"], agent, "large.bin", stream)
        require(file["size"] == size, "Large file size changed")
        require(download(space, file["key"]) == (size, digest), "Large file bytes changed")
        STATE.write_text(json.dumps(state | {"file": file["key"], "size": size, "sha256": digest}))
        print(f"PASS large_file_round_trips {size} bytes SHA256 {digest}", flush=True)
    finally:
        payload.unlink(missing_ok=True)


def persist():
    state = json.loads(STATE.read_text())
    require(cli("space", "show", state["space"])["key"] == state["space"], "Space did not persist")
    require(
        download(state["space"], state["file"]) == (state["size"], state["sha256"]), "Persistent file bytes changed"
    )
    print("PASS restart_preserves_sign_in_space_and_file_bytes", flush=True)


def revoke():
    state = json.loads(STATE.read_text())
    name = "Deployment probe"
    require(name in json.dumps(cli("agent", "list")), "Agent sign-in is missing")
    cli("agent", "revoke", name, "--yes")
    api("GET", "/api/v1/spaces", token=state["agent"], expected=401)
    cli("logout")
    print("PASS revoked_agent_is_refused_and_person_logs_out", flush=True)


def rate():
    codes = []
    for _ in range(SETTINGS["attempts"]):
        request = urllib.request.Request(
            ORIGIN + "/api/v1/sign-in",
            json.dumps(
                {
                    "username": PEOPLE[0]["username"],
                    "password": PEOPLE[0]["password"] if SETTINGS["valid"] else "wrong-password",
                }
            ).encode(),
            {"Content-Type": "application/json"},
        )
        try:
            response = urllib.request.urlopen(request, context=CONTEXT, timeout=20)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            codes.append(response.status)
            if response.status == 200:
                token = json.load(response)["data"]["access_token"]
                api("DELETE", "/api/v1/sign-in", token=token)
    print(json.dumps(codes))


def trust():
    with sync_playwright() as driver:
        for trusted in (False, True):
            environment = os.environ | {"HOME": "/root" if trusted else "/untrusted"}
            browser = driver.chromium.launch(
                executable_path="/usr/bin/chromium", args=["--no-sandbox"], env=environment
            )
            try:
                page = browser.new_page()
                try:
                    response = page.goto(ORIGIN + "/readyz")
                except Exception as error:
                    require(
                        not trusted and "ERR_CERT_AUTHORITY_INVALID" in str(error), "Unexpected browser TLS failure"
                    )
                else:
                    require(trusted and response.status == 200, "Browser accepted an untrusted authority")
            finally:
                browser.close()
    print("PASS browser_requires_supplied_authority", flush=True)


def loopback():
    process = subprocess.Popen(
        [
            "ssh",
            "-i",
            "/work/ssh-key",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ExitOnForwardFailure=yes",
            "-N",
            "-L",
            "8080:127.0.0.1:8080",
            "root@" + SETTINGS["server"],
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    try:
        for _ in range(30):
            try:
                with urllib.request.urlopen("http://127.0.0.1:8080/readyz", timeout=2) as response:
                    require(response.status == 200, "Forwarded readiness failed")
                break
            except OSError:
                time.sleep(0.2)
        require(process.poll() is None, "SSH forwarding failed")
        if SETTINGS["loopback_origin"]:
            login(0, origin="http://127.0.0.1:8080", home="/work/loopback")
            cli("space", "list", home="/work/loopback")
            cli("logout", home="/work/loopback")
            login(0, home="/work/domain-refused", refused=True)
        else:
            login(0, home="/work/domain-restored")
            cli("logout", home="/work/domain-restored")
            login(0, origin="http://127.0.0.1:8080", home="/work/loopback-refused", refused=True)
        print("PASS sign_in_accepts_only_configured_origin_over_ssh_forward", flush=True)
    finally:
        process.terminate()
        process.communicate(timeout=10)


try:
    Path("/work").mkdir(exist_ok=True)
    globals()[SETTINGS["operation"]]()
except Exception as error:
    print("FAIL " + type(error).__name__ + ": " + CLIENTS.redact(str(error)), flush=True)
    sys.exit(1)
finally:
    CLIENTS.close()
