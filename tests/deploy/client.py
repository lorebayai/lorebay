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
from clients import Agent, Clients, require
from journeys import run_journeys


SETTINGS = json.load(sys.stdin)
ORIGIN = SETTINGS["origin"]
PEOPLE = SETTINGS["people"]
CA = "/inputs/authority.pem"
STATE = Path("/work/state.json")
CLIENTS = Clients(ORIGIN, PEOPLE, "/work", authority=CA, chromium="/usr/bin/chromium")
CONTEXT = CLIENTS.context
api = CLIENTS.api
cli = CLIENTS.cli


def login(person, device=False, wrong=False, origin=None, home=None, refused=False):
    print(
        "BEGIN cli_sign_in " + ("device" if device else "authorization") + (" refusal" if refused else ""), flush=True
    )
    return CLIENTS.login(person, device, wrong, origin, home, refused)


def download(space, key):
    return CLIENTS.download(cli("file", "url", space, key)["url"])


def journeys():
    people = {int(number): group for number, group in SETTINGS["journey_people"].items()}
    run_journeys(ORIGIN, people, Path("/work/journeys"), authority=CA, chromium="/usr/bin/chromium",
                 credentials=CLIENTS.secrets)


def large():
    login(0)
    space = cli("space", "create", "--name", "Streaming and persistence", "--summary", "Deployment operation")["key"]
    agent = Agent(CLIENTS, 0, "Deployment probe")
    state = {"space": space, "agent": agent.token}
    task = agent.call("create_task", space=space, title="Large file persistence")
    size = 1073741825
    payload = Path("/work/large.bin")
    try:
        with payload.open("wb") as stream:
            stream.write(os.urandom(65536))
            stream.truncate(size)
        with payload.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        registered = agent.call("create_file", task_id=task["task_id"], file_name="large.bin",
                                summary="Streaming probe")
        with payload.open("rb") as stream:
            CLIENTS.upload(registered, stream)
        file = agent.call("complete_file", task_id=task["task_id"], key=registered["key"])
        require(file["size"] == size, "Large file size changed")
        require(download(space, file["key"]) == (size, digest), "Large file bytes changed")
        STATE.write_text(json.dumps(state | {"file": file["key"], "size": size, "sha256": digest}))
        STATE.chmod(0o600)
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
    Agent(CLIENTS, 0, name, token=state["agent"]).call("list_spaces", http_status=401)
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
