import base64
import hashlib
import http.client
import json
import os
import queue
import re
import secrets
import ssl
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any

from playwright.sync_api import expect, sync_playwright


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def redact(message, credentials=()):
    for secret in sorted(credentials, key=len, reverse=True):
        message = message.replace(secret, "<redacted>")
    message = re.sub(r"https?://\S+", "<url>", message)
    message = re.sub(r"eyJ[A-Za-z0-9_.-]+", "<token>", message)
    return re.sub(r"\b[A-Z2-9]{4}-[A-Z2-9]{4}\b", "<device-code>", message)


class Clients:
    def __init__(self, origin, people, root, executable="lorebay", authority=None, chromium=None):
        self.origin = origin.rstrip("/")
        self.people = people
        self.root = Path(root)
        self.executable = str(executable)
        self.authority = str(authority) if authority else None
        self.chromium = chromium
        self.context = ssl.create_default_context(cafile=self.authority)
        self.secrets = [person["password"] for person in people]
        self.driver = None
        self.browser = None

    def close(self):
        try:
            if self.browser:
                self.browser.close()
        finally:
            if self.driver:
                self.driver.stop()

    def redact(self, message):
        return redact(message, self.secrets)

    def page(self):
        if self.browser is None:
            self.driver = sync_playwright().start()
            self.browser = self.driver.chromium.launch(executable_path=self.chromium, args=["--no-sandbox"])
        context = self.browser.new_context()
        return context, context.new_page()

    def environment(self, person=0, home=None):
        environment = os.environ | {"LOREBAY_HOME": str(home or self.root / f"person-{person}")}
        if self.authority:
            environment["SSL_CERT_FILE"] = self.authority
        return environment

    def api(self, method, path, body=None, token=None, expected=200, form=False, headers=None) -> Any:
        content_type = "application/x-www-form-urlencoded" if form else "application/json"
        headers = {"Content-Type": content_type} | (headers or {})
        if token:
            headers["Authorization"] = "Bearer " + token
        data = None if body is None else (urllib.parse.urlencode(body) if form else json.dumps(body)).encode()
        request = urllib.request.Request(self.origin + path, data, headers, method=method)
        try:
            response = urllib.request.urlopen(request, context=self.context, timeout=120)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read()
            require(response.status == expected, f"{method} {path}: HTTP {response.status}, expected {expected}")
            return json.loads(raw) if raw else None

    def cli(self, *args, person=0, home=None, failure=None) -> Any:
        child = subprocess.run(
            [self.executable, *map(str, args), "--output", "json"],
            env=self.environment(person, home), capture_output=True, text=True, timeout=60, check=False,
        )
        if failure is not None:
            require(child.returncode != 0, f"CLI {args[:2]} unexpectedly succeeded")
            error = json.loads(child.stderr)
            require(error.get("status") in failure, f"CLI {args[:2]} returned {error.get('status')}")
            return error
        require(child.returncode == 0, f"CLI {args[:2]} failed: {self.redact(child.stderr)}")
        return json.loads(child.stdout)["data"] if child.stdout.strip() else None

    def browser_sign_in(self, page, person, wrong=False):
        page.get_by_label("Username", exact=True).fill(self.people[person]["username"])
        password = "wrong-password" if wrong else self.people[person]["password"]
        page.get_by_label("Password", exact=True).fill(password)
        page.get_by_role("button", name="Sign in", exact=True).click()
        if wrong:
            expect(page.locator("#message")).to_have_text("The request failed. Check the link or sign in again.")
            expect(page.locator("#login")).to_be_visible()
        else:
            expect(page.locator("#login")).to_be_hidden()

    def login(self, person, device=False, wrong=False, origin=None, home=None, refused=False):
        args = [self.executable, "login", origin or self.origin, "--output", "json"]
        args += ["--device"] if device else ["--no-browser"]
        child = subprocess.Popen(
            args, env=self.environment(person, home), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        assert child.stdout is not None and child.stderr is not None
        stdout, stderr = child.stdout, child.stderr
        pending = queue.Queue()

        def read_lines():
            for line in stderr:
                pending.put(line)
            pending.put(None)

        reader = threading.Thread(target=read_lines, daemon=True)
        reader.start()
        context = None
        try:
            lines = []
            link = code = None
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline:
                try:
                    line = pending.get(timeout=1)
                except queue.Empty:
                    continue
                if line is None:
                    break
                lines.append(line)
                found = re.search(r"https?://\S+", line)
                if found:
                    link = found.group()
                found = re.search(r"Enter this code: (\S+)", line)
                if found:
                    code = found.group(1)
                if link and (not device or code):
                    break
            if refused:
                child.wait(timeout=30)
                reader.join(timeout=5)
                while not pending.empty():
                    line = pending.get()
                    if line:
                        lines.append(line)
                require(
                    child.returncode != 0 and "endpoint on another server" in "".join(lines).lower(),
                    "Wrong-origin sign-in was not refused",
                )
                return
            require(link is not None and (not device or code), "CLI did not supply its sign-in page")
            assert link is not None
            context, page = self.page()
            page.goto(link)
            if wrong:
                self.browser_sign_in(page, person, wrong=True)
            self.browser_sign_in(page, person)
            if device:
                assert code is not None
                page.get_by_label("User code", exact=True).fill(code)
                page.get_by_role("button", name="Review code", exact=True).click()
            page.get_by_role("button", name="Approve", exact=True).click()
            if device:
                expect(page.locator("#message")).to_have_text(
                    "Sign-in approved. Return to your terminal. This page is signed out. You may close it."
                )
            child.wait(timeout=30)
            require(child.returncode == 0, "CLI sign-in did not complete")
        finally:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=10)
            reader.join(timeout=5)
            stdout.close()
            stderr.close()
            if context:
                context.close()

    def agent_sign_in(self, person=0, name="Deployment probe"):
        verifier = secrets.token_urlsafe(48)
        challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
        state = secrets.token_urlsafe(32)
        codes = []

        class Callback(BaseHTTPRequestHandler):
            def do_GET(self):
                address = urllib.parse.urlsplit(self.path)
                if address.path != "/callback":
                    self.send_response(404)
                    self.end_headers()
                    return
                codes.append(urllib.parse.parse_qs(address.query))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"Authorization received")

            def log_message(self, format, *args):
                pass

        callback = HTTPServer(("127.0.0.1", 0), Callback)
        thread = threading.Thread(target=callback.serve_forever, daemon=True)
        thread.start()
        context = None
        try:
            redirect = f"http://127.0.0.1:{callback.server_port}/callback"
            registered = self.api("POST", "/oauth/register", {
                "client_name": name, "redirect_uris": [redirect], "token_endpoint_auth_method": "none",
            }, expected=201)
            parameters = {
                "client_id": registered["client_id"], "redirect_uri": redirect, "response_type": "code",
                "code_challenge": challenge, "code_challenge_method": "S256", "state": state,
            }
            metadata = self.api("GET", "/.well-known/oauth-authorization-server")
            context, page = self.page()
            page.goto(metadata["authorization_endpoint"] + "?" + urllib.parse.urlencode(parameters))
            self.browser_sign_in(page, person)
            page.get_by_label("Agent name", exact=True).fill(name)
            page.get_by_role("button", name="Approve", exact=True).click()
            expect(page.get_by_text("Authorization received", exact=True)).to_be_visible()
            require(len(codes) == 1 and "code" in codes[0] and codes[0].get("state") == [state],
                    "Agent callback did not receive its authorization")
            token = self.api("POST", "/oauth/token", {
                "grant_type": "authorization_code", "client_id": registered["client_id"],
                "redirect_uri": redirect, "code": codes[0]["code"][0], "code_verifier": verifier,
            }, form=True)["access_token"]
            self.secrets.append(token)
            return token
        finally:
            if context:
                context.close()
            callback.shutdown()
            callback.server_close()
            thread.join()

    def upload(self, registered, payload):
        url = urllib.parse.urlsplit(registered["upload_url"])
        if url.scheme == "https":
            connection = http.client.HTTPSConnection(url.hostname, url.port, context=self.context, timeout=180)
        else:
            require(url.scheme == "http", "Unsupported upload URL")
            connection = http.client.HTTPConnection(url.hostname, url.port, timeout=180)
        try:
            connection.request(
                registered["method"], url.path + "?" + url.query, payload, registered["headers"], encode_chunked=True,
            )
            response = connection.getresponse()
            response.read()
            require(response.status == 201, f"File upload: HTTP {response.status}")
        finally:
            connection.close()

    def download(self, url):
        digest = hashlib.sha256()
        size = 0
        with urllib.request.urlopen(url, context=self.context, timeout=180) as response:
            while chunk := response.read(4 * 1024 * 1024):
                digest.update(chunk)
                size += len(chunk)
        return size, digest.hexdigest()


class Agent:
    def __init__(self, clients, person, name):
        self.clients = clients
        self.name = name
        self.token = clients.agent_sign_in(person, name)

    def call(self, tool, *, failure=None, http_status=200, **arguments):
        response = self.clients.api("POST", "/mcp", {
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments, "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "Team journey", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {},
            }},
        }, self.token, expected=http_status, headers={
            "Accept": "application/json, text/event-stream", "MCP-Protocol-Version": "2026-07-28",
            "Mcp-Method": "tools/call", "Mcp-Name": tool,
        })
        if http_status != 200:
            return response
        require("error" not in response, f"MCP {tool} returned a protocol error")
        result = response["result"]
        data = result["structuredContent"]
        if failure:
            require(result.get("isError") and data.get("code") == failure,
                    f"MCP {tool} did not refuse with {failure}: {data.get('code')}")
        else:
            require(not result.get("isError"), f"MCP {tool} failed: {data}")
        return data
