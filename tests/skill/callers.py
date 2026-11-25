import json
import os
import subprocess
from contextlib import ExitStack, contextmanager
from dataclasses import dataclass

from clients import Agent, Clients, require
from leases import HomeLock


@dataclass
class Callers:
    clients: Clients
    agents: dict[str, Agent]
    locks: dict[str, int]


@contextmanager
def home_leases(config):
    paths = [entry["lock"] for section in ("people", "script_agents", "homes") for entry in config[section].values()]
    with ExitStack() as stack:
        yield {path: stack.enter_context(HomeLock(path)) for path in sorted(paths)}


class PersistentClients(Clients):
    def __init__(self, config, directory, locks):
        self.profiles = [config["people"][name] for name in ("owner", "other", "admin")]
        self.locks = locks
        super().__init__(config["origin"], self.profiles, directory, config["cli"], config["authority"])

    def environment(self, person=0, home=None):
        require(home is None, "A persistent CLI home cannot be overridden")
        profile = self.profiles[person]
        return super().environment(person, profile["home"]) | {
            "LOREBAY_TEST_HOME_LOCK_FD": str(self.locks[profile["lock"]]),
        }

    def process_options(self, person=0, home=None):
        return {"pass_fds": (self.locks[self.profiles[person]["lock"]],)}


class CommandAPI:
    def __init__(self, origin, profile, descriptor):
        self.origin, self.profile, self.descriptor = origin, profile, descriptor

    def api(self, method, path, body=None, token=None, expected=200, **_options):
        require(method == "POST" and path == "/mcp" and expected == 200, "Script agents accept MCP requests only")
        environment = os.environ | {
            "LOREBAY_TEST_HOME_LOCK_FD": str(self.descriptor), "LOREBAY_TEST_EXPECTED_ORIGIN": self.origin,
        }
        child = subprocess.run(self.profile["command"], input=json.dumps(body), capture_output=True, text=True,
                               env=environment, pass_fds=(self.descriptor,), timeout=120, check=False)
        require(child.returncode == 0, "Script-agent command failed; check its authorization outside the suite")
        return json.loads(child.stdout)


class ScriptAgent(Agent):
    def __init__(self, api):
        self.clients = api
        self.name = "Script agent"
        self.token = None


def prepare_callers(config, directory, locks):
    clients = PersistentClients(config, directory, locks)
    agents: dict[str, Agent] = {
        name: ScriptAgent(CommandAPI(config["origin"], profile, locks[profile["lock"]]))
        for name, profile in config["script_agents"].items()
    }
    return Callers(clients, agents, locks=locks)
