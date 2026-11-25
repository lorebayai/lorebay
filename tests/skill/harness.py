import hashlib
import json
import os
import queue
import re
import signal
import subprocess
import threading
import time
from pathlib import Path

from configuration import WORKSPACE, RunInterrupted, require


def stop(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=5)
    finally:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def scrub(value, clean):
    if isinstance(value, dict):
        return {key: ("<redacted>" if key.lower() in {
            "access_token", "refresh_token", "password", "authorization", "client_secret",
        } else scrub(item, clean)) for key, item in value.items()}
    if isinstance(value, list):
        return [scrub(item, clean) for item in value]
    return clean(value) if isinstance(value, str) else value


class Harness:
    def __init__(self, home, project, timeout, clean, lock_fd=None):
        self.home, self.project, self.timeout, self.clean = home, project, timeout, clean
        self.session = None
        self.events = []
        self.tools = set()
        self.loaded = False
        self.skill_reads = {}
        self.skill = home["skill"]
        self.digest = None
        self.lock_fd = lock_fd

    def environment(self):
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith(("CLAUDE", "CODEX", "ANTHROPIC", "OPENAI", "LOREBAY"))}
        environment.update(self.home["environment"])
        if self.lock_fd is not None:
            environment["LOREBAY_TEST_HOME_LOCK_FD"] = str(self.lock_fd)
        return environment

    def inherited_fds(self):
        return (self.lock_fd,) if self.lock_fd is not None else ()

    def turn(self, text):
        self.events.append({"person": self.clean(text)})
        installed = Path(self.skill).read_bytes()
        self.digest = hashlib.sha256(installed).hexdigest()
        require(self.digest == hashlib.sha256((WORKSPACE / "skill/lorebay/SKILL.md").read_bytes()).hexdigest(),
                "Installed skill differs from this checkout")
        command = list(self.home["command"])
        if self.home["harness"] == "claude":
            command += ["--print", "--verbose", "--output-format", "stream-json", "--permission-mode", "auto"]
            if self.session:
                command += ["--resume", self.session]
        else:
            command += ["--no-daemon", "exec"]
            if self.session:
                command += ["resume", self.session]
            else:
                command += ["--skip-git-repo-check", "--sandbox", "workspace-write"]
            command += ["--json", "-"]
        child = subprocess.Popen(command, cwd=self.project, env=self.environment(), start_new_session=True,
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                 pass_fds=self.inherited_fds())
        stdout = stderr = ""
        try:
            try:
                stdout, stderr = child.communicate(text, timeout=self.timeout)
            except (subprocess.TimeoutExpired, RunInterrupted):
                stop(child)
                stdout, stderr = child.communicate(timeout=5)
                raise
            finally:
                for line in stdout.splitlines():
                    try:
                        event = json.loads(line)
                    except ValueError:
                        event = {"diagnostic": line}
                    if not isinstance(event, dict):
                        event = {"diagnostic": line}
                    self.inspect(event)
                    self.events.append(scrub(event, self.clean))
            require(child.returncode == 0, "Harness failed: " + self.clean(stderr))
            require(self.session, "Harness did not identify its continuing session")
        finally:
            stop(child)
            for stream in (child.stdin, child.stdout, child.stderr):
                if stream:
                    stream.close()

    def inspect(self, event):
        if event.get("session_id"):
            self.session = event["session_id"]
        if event.get("type") == "thread.started":
            self.session = event["thread_id"]
        if event.get("type") == "system" and event.get("subtype") == "init":
            prefix = "mcp__lorebay__"
            self.tools.update(name.removeprefix(prefix) for name in event.get("tools", []) if name.startswith(prefix))
        raw = json.dumps(event)
        item = event.get("item", {})
        if (event.get("type") == "item.completed" and item.get("type") == "command_execution"
                and item.get("exit_code") == 0 and self.skill in item.get("command", "")
                and "# Lorebay" in item.get("aggregated_output", "")):
            self.loaded = True
        for block in event.get("message", {}).get("content", []):
            if block.get("type") == "tool_use":
                if (block.get("name") == "Skill" and block.get("input", {}).get("skill") == "lorebay:lorebay"
                        or self.skill in json.dumps(block.get("input", {}))):
                    self.skill_reads[block.get("id")] = block.get("name")
            if (block.get("type") == "tool_result" and block.get("tool_use_id") in self.skill_reads
                    and not block.get("is_error") and (self.skill_reads[block.get("tool_use_id")] == "Skill"
                                                       or "# Lorebay" in raw or self.skill in raw)):
                self.loaded = True

    def codex_tools(self):
        self.tools.update(codex_inventory(self.home["command"], self.environment(), self.project,
                                          pass_fds=self.inherited_fds()))

    def record(self):
        return {"session": self.session, "harness": self.home["harness"], "skill": self.skill,
                "sha256": self.digest, "loaded": self.loaded, "tools": sorted(self.tools), "events": self.events}


def codex_inventory(command, environment, project, *, pass_fds=()):
    tools = set()
    child = subprocess.Popen([*command, "app-server", "--stdio"], cwd=project,
                             env=environment, start_new_session=True, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, pass_fds=pass_fds)
    assert child.stdin is not None and child.stdout is not None
    stdin, stdout = child.stdin, child.stdout
    incoming = queue.Queue()

    def read():
        for line in stdout:
            incoming.put(line)
        incoming.put(None)

    reader = threading.Thread(target=read, daemon=True)
    reader.start()

    def request(number, method, params):
        stdin.write(json.dumps({"id": number, "method": method, "params": params}) + "\n")
        stdin.flush()
        deadline = time.monotonic() + 60
        while True:
            line = incoming.get(timeout=max(0, deadline - time.monotonic()))
            require(line is not None, "Harness inventory ended before responding")
            response = json.loads(line)
            if response.get("id") == number:
                require("error" not in response, "Harness inventory request failed")
                return response["result"]

    try:
        request(1, "initialize", {"clientInfo": {"name": "skill-acceptance", "version": "1"}})
        stdin.write('{"method":"initialized"}\n')
        stdin.flush()
        cursor, number = None, 2
        while True:
            result = request(number, "mcpServerStatus/list", {"cursor": cursor, "serverName": "lorebay"})
            for server in result["data"]:
                if server["name"] == "lorebay":
                    require(not server.get("toolsError"), "Harness tool discovery failed")
                    tools.update(server["tools"])
            cursor = result.get("nextCursor")
            if not cursor:
                break
            number += 1
    finally:
        stop(child)
        reader.join(timeout=5)
        stdin.close()
        stdout.close()
    return tools



def sanitize(text, credentials):
    for secret in sorted(set(credentials), key=len, reverse=True):
        if secret:
            text = text.replace(secret, "<redacted>")
    text = re.sub(r"https?://[^\s\"<>]+", "<url>", text)
    text = re.sub(r"(?:eyJ|gh[pousr]_|github_pat_|sk-)[A-Za-z0-9_.-]+", "<token>", text)
    text = re.sub(r"(?i)Bearer\s+[A-Za-z0-9._~+/-]+=*", "Bearer <redacted>", text)
    text = re.sub(r"(?i)((?:access_token|refresh_token|password|authorization)\"?\s*[:=]\s*\")[^\"]*",
                  r"\1<redacted>", text)
    return text
