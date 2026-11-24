import getpass
import hashlib
import json
import os
import re
import secrets
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import uuid
from pathlib import Path


HERE = Path(__file__).resolve().parent
REPOSITORY = "lorebayai/lorebay"
IMAGE = "ghcr.io/lorebayai/lorebayd"
LABEL = "io.lorebay.deploy"
VERSION_PATTERN = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?"
ARCHIVES = [f"{name}-{arch}-unknown-linux-musl.tar.gz"
            for name in ("lorebay", "lorebayd") for arch in ("x86_64", "aarch64")]


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class Suite:
    def __init__(self, configuration):
        self.config = json.loads(configuration.read_text())
        self.prefix = "lorebay-deploy-" + uuid.uuid4().hex[:12]
        self.data = Path("/srv") / self.prefix
        self.data_owned = False
        self.environment = dict(os.environ)
        self.secrets = []
        self.output = []
        self.containers = []
        self.networks = []
        self.images = []
        self.compose_args = None
        self.port = None
        self.stage = "configuration"
        self.people = [{"username": name, "password": secrets.token_urlsafe(32)} for name in self.config["people"]]
        self.secrets += [person["password"] for person in self.people]
        self.domain = self.config["domain"]
        require(len(self.people) == 2 and self.people[0]["username"] != self.people[1]["username"], "Configure two different people")
        require(all(re.fullmatch(r"[a-zA-Z0-9_-]+", p["username"]) for p in self.people), "Invalid test username")
        require(re.fullmatch(r"[a-z0-9.-]+", self.domain) is not None, "Invalid domain")
        sources = {key: Path(self.config[key]).expanduser().resolve(strict=True)
                   for key in ("certificate", "key", "authority")}
        parent = Path("/tmp") / getpass.getuser() / "lorebay"
        self.created_parents = [p for p in (parent.parent, parent) if not p.exists()]
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix=self.prefix + "-", dir=parent))
        try:
            for key, source in sources.items():
                shutil.copyfile(source, self.root / (key + ".pem"))
                (self.root / (key + ".pem")).chmod(0o600)
            self.bundle = self.root / "bundle"
            self.release = self.root / "release"
            self.release.mkdir()
        except BaseException:
            shutil.rmtree(self.root)
            for path in reversed(self.created_parents):
                path.rmdir()
            raise
        self.origin = "https://" + self.domain

    def clean_text(self, value):
        for secret in self.secrets:
            value = value.replace(secret, "<redacted>")
        value = re.sub(r"(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]+", "<github-token>", value)
        value = re.sub(r"eyJ[A-Za-z0-9_.-]+", "<access-token>", value)
        value = re.sub(r"\$argon2\S+", "<password-hash>", value)
        value = re.sub(r"((?:https?://[^/\s]+)?/[^\s?\"']+)\?[^\s\"']+", r"\1?<redacted>", value)
        return value

    def say(self, value):
        value = self.clean_text(value)
        self.output.append(value)
        print(value, flush=True)

    def run(self, *args, check=True, input=None, timeout=600, env=None):
        result = subprocess.run([str(arg) for arg in args], input=input, text=True,
                                capture_output=True, timeout=timeout, env=env or self.environment)
        if check and result.returncode:
            raise RuntimeError(f"Command failed ({result.returncode}): {list(map(str, args[:5]))}\n{result.stdout}\n{result.stderr}")
        return result

    def docker(self, *args, **kwargs):
        return self.run("docker", *args, **kwargs)

    def execute(self, name, *args, **kwargs):
        return self.docker("exec", "-i", name, *args, **kwargs)

    def copy(self, source, name, destination):
        self.docker("cp", source, f"{name}:{destination}")

    def container(self, suffix, image, *args, command=None):
        name = self.prefix + "-" + suffix
        self.containers.append(name)
        self.docker("run", "-d", "--name", name, "--label", f"{LABEL}={self.prefix}",
                    *args, image, *(command or []))
        return name

    def network(self, suffix):
        name = self.prefix + "-" + suffix
        self.networks.append(name)
        self.docker("network", "create", "--label", f"{LABEL}={self.prefix}", name)
        return name

    def image_present(self, image):
        return self.docker("image", "inspect", image, check=False).returncode == 0

    def pull(self, image):
        if not self.image_present(image):
            self.images.append(image)
            self.docker("pull", image)

    def build(self, role, base=None):
        image = self.prefix + ":" + (base.replace(":", "-") if base else role)
        self.images.append(image)
        args = ["build", "--label", f"{LABEL}={self.prefix}", "-t", image,
                "-f", HERE / ("Dockerfile." + role)]
        if base:
            self.pull(base)
            args += ["--build-arg", "BASE=" + base]
        self.say("Building isolated " + image)
        self.docker(*args, HERE, timeout=1800)
        return image

    def compose(self, *args, **kwargs):
        return self.run(*self.compose_args, *args, **kwargs)

    def address(self, name, network):
        details = json.loads(self.docker("inspect", name).stdout)[0]
        return details["NetworkSettings"]["Networks"][network]["IPAddress"]

    def ready(self):
        for _ in range(120):
            result = self.run("curl", "--fail", "--silent", "--show-error", "--noproxy", "*",
                              "--cacert", self.root / "authority.pem", "--resolve",
                              f"{self.domain}:{self.port}:127.0.0.1",
                              f"https://{self.domain}:{self.port}/readyz", check=False, timeout=5)
            if result.returncode == 0:
                require(json.loads(result.stdout)["data"] == "ok", "Unexpected readiness response")
                return
            time.sleep(1)
        raise RuntimeError("HTTPS readiness did not arrive")

    def choose_port(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            self.port = listener.getsockname()[1]

    def fingerprint(self, authority=None):
        context = ssl.create_default_context(cafile=str(authority or self.root / "authority.pem"))
        with socket.create_connection(("127.0.0.1", self.port), timeout=10) as connection:
            with context.wrap_socket(connection, server_hostname=self.domain) as tls:
                return hashlib.sha256(tls.getpeercert(binary_form=True)).hexdigest()

    def credentials(self):
        account = self.config.get("registry", {}).get("github_account")
        require(account, "Configure the existing GitHub account name")
        token = self.run("gh", "auth", "token", "--hostname", "github.com", "--user", account).stdout.strip()
        require(bool(token), "GitHub sign-in is required")
        self.secrets.append(token)
        self.environment["GH_TOKEN"] = token
        directory = self.root / "docker"
        directory.mkdir(mode=0o700)
        helper = self.root / ("docker-credential-" + self.prefix)
        helper.write_text(
            "#!/usr/bin/env python3\nimport json, os, sys\n"
            "if sys.argv[1] == 'get':\n"
            "    sys.stdin.read()\n"
            "    print(json.dumps({'Username': os.environ['DEPLOY_GITHUB_ACCOUNT'], 'Secret': os.environ['GH_TOKEN']}))\n"
            "else:\n    sys.exit(1)\n"
        )
        helper.chmod(0o700)
        (directory / "config.json").write_text(json.dumps({"credHelpers": {"ghcr.io": self.prefix}}))
        self.environment |= {"DOCKER_CONFIG": str(directory), "PATH": str(self.root) + os.pathsep + os.environ["PATH"],
                             "DEPLOY_GITHUB_ACCOUNT": account}

    def release_files(self):
        self.stage = "release_files"
        self.credentials()
        target = self.config["target"]
        require(("version" in target) != ("directory" in target), "Select a version or a local release directory")
        hosted = "directory" not in target
        if hosted:
            self.version = target["version"]
            require(re.fullmatch("v" + VERSION_PATTERN, self.version), "Invalid release version")
            metadata = json.loads(self.run("gh", "release", "view", self.version, "--repo", REPOSITORY,
                                           "--json", "assets,tagName,isDraft").stdout)
            names = {asset["name"] for asset in metadata["assets"]}
            require(names == {*ARCHIVES, "lorebay-deploy.tar.gz", "SHA256SUMS"}, "Unexpected release asset set")
            self.run("gh", "release", "download", self.version, "--repo", REPOSITORY, "--dir", self.release)
            self.say(f"Release {self.version}; draft={metadata['isDraft']}")
        else:
            source = Path(target["directory"]).expanduser().resolve(strict=True)
            for name in (*ARCHIVES, "lorebay-deploy.tar.gz", "SHA256SUMS", "image.tar"):
                shutil.copyfile(source / name, self.release / name)
            self.version = None
        if hosted:
            self.run("gh", "attestation", "verify", self.release / "SHA256SUMS", "--repo", REPOSITORY)
            self.say("PASS release_checksums_match_attestation")
        sums = {}
        for line in (self.release / "SHA256SUMS").read_text().splitlines():
            match = re.fullmatch(r"([a-fA-F0-9]{64}) [ *]([^/]+)", line)
            require(match is not None, "Malformed release checksum entry")
            digest, name = match.groups()
            require(name not in sums and name not in (".", ".."), "Duplicate or invalid release checksum entry")
            sums[name] = digest.lower()
        expected = {*ARCHIVES, "lorebay-deploy.tar.gz"} | (set() if hosted else {"image.tar"})
        require(set(sums) == expected, "SHA256SUMS must cover exactly the release files")
        for name, digest in sums.items():
            with (self.release / name).open("rb") as stream:
                require(hashlib.file_digest(stream, "sha256").hexdigest() == digest, f"Checksum mismatch: {name}")
            if hosted:
                self.run("gh", "attestation", "verify", self.release / name, "--repo", REPOSITORY)
                self.say("PASS release_archive_matches_attestation " + name)
        if hosted:
            tag = IMAGE + ":" + self.version[1:]
            details = json.loads(self.docker("buildx", "imagetools", "inspect", tag,
                                            "--format", "{{json .Manifest}}").stdout)
            digest = details["digest"]
            require(re.fullmatch(r"sha256:[a-f0-9]{64}", digest), "Invalid registry manifest digest")
            self.server_image = IMAGE + "@" + digest
            self.run("gh", "attestation", "verify", "oci://" + self.server_image, "--repo", REPOSITORY)
            self.pull(self.server_image)
            self.say("Verified release image " + self.server_image)
        else:
            with tarfile.open(self.release / "image.tar") as archive:
                manifest = json.load(archive.extractfile("manifest.json"))
                require(len(manifest) == 1, "Local image.tar must contain exactly one image")
            before = set(self.docker("image", "ls", "-aq", "--no-trunc").stdout.split())
            local = self.prefix + ":local"
            self.images.append(local)
            self.run("skopeo", "copy", "--format", "v2s2", "docker-archive:" + str(self.release / "image.tar"),
                     "docker-daemon:" + local)
            self.server_image = json.loads(self.docker("image", "inspect", local).stdout)[0]["Id"]
            if self.server_image not in before:
                self.images.insert(len(self.images) - 1, self.server_image)
            self.say("Local release: SHA256SUMS verified; hosted attestations do not apply")
        self.bundle.mkdir()
        with tarfile.open(self.release / "lorebay-deploy.tar.gz") as archive:
            archive.extractall(self.bundle, filter="data")
        for name in ("lorebay", "lorebayd"):
            destination = self.root / name
            destination.mkdir()
            with tarfile.open(self.release / f"{name}-x86_64-unknown-linux-musl.tar.gz") as archive:
                archive.extractall(destination, filter="data")
        cli_version = self.run(self.root / "lorebay/lorebay", "--version").stdout.strip()
        require(re.fullmatch("lorebay " + VERSION_PATTERN, cli_version), "Invalid CLI archive version")
        if hosted:
            require(cli_version == "lorebay " + self.version[1:], "CLI version differs from target")
        else:
            self.version = "v" + cli_version.split()[1]
        require(self.run(self.root / "lorebayd/lorebayd", "--version").stdout.strip() == "lorebayd " + self.version[1:], "Server archive version differs from target")
        self.say("PASS release_files_pass_integrity_checks")

    def prepare(self):
        self.stage = "environment"
        self.pull("debian:12")
        self.pull("alpine:3")
        self.client_image = self.build("client")
        self.server_images = {base: self.build("server", base) for base in ("debian:12", "ubuntu:24.04")}
        self.pull("debian:13")
        self.run("sudo", "-n", "mkdir", "-m", "0755", self.data)
        self.data_owned = True
        users = []
        for person in self.people:
            password_hash = self.run(self.root / "lorebayd/lorebayd", "hash-password", input=person["password"] + "\n").stdout.strip()
            self.secrets.append(password_hash)
            users.append({"username": person["username"], "password_hash": password_hash})
        (self.root / "users.yaml").write_text(json.dumps(users))
        (self.root / "users.yaml").chmod(0o600)
        for name, domain in (("replacement", self.domain), ("other", "other.example.com")):
            self.run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                     "-subj", "/CN=" + domain, "-addext", "subjectAltName=DNS:" + domain,
                     "-keyout", self.root / (name + ".key"), "-out", self.root / (name + ".pem"))
        self.say("PASS isolated_environment_built " + self.prefix)

    def clients(self, network, server):
        address = self.address(server, network)
        clients = []
        for index in range(2):
            name = self.container(f"{self.method}-client-{index}", self.client_image,
                                  "--network", network, "--add-host", f"{self.domain}:{address}")
            self.execute(name, "mkdir", "-p", "/inputs", "/work", "/root/.pki/nssdb", "/untrusted")
            self.copy(self.root / "authority.pem", name, "/inputs/authority.pem")
            self.copy(self.root / "lorebay/lorebay", name, "/usr/local/bin/lorebay")
            self.execute(name, "certutil", "-N", "--empty-password", "-d", "sql:/root/.pki/nssdb")
            self.execute(name, "certutil", "-A", "-d", "sql:/root/.pki/nssdb", "-t", "C,,", "-n", "deployment-ca", "-i", "/inputs/authority.pem")
            clients.append(name)
        self.client_a, self.client_b = clients
        require(self.address(clients[0], network) != self.address(clients[1], network), "Clients share an address")

    def drive(self, operation, client=None, **options):
        settings = {"operation": operation, "origin": self.origin, "people": self.people} | options
        result = self.execute(client or self.client_a, "python3", "-B", "/driver.py", input=json.dumps(settings))
        self.say(result.stdout.strip())
        return result.stdout

    def operation_checks(self):
        self.stage = self.method + ": product"
        self.drive("trust")
        self.drive("product")
        self.stage = self.method + ": per-caller limits"
        first = json.loads(self.drive("rate", attempts=25, valid=False))
        second = json.loads(self.drive("rate", client=self.client_b, attempts=1, valid=True))
        require(429 in first and second == [200], "Rate limits do not separate callers")
        self.say("PASS per_caller_limits_refuse_one_client_and_accept_another")
        self.stage = self.method + ": large file"
        directories = ["/var/lib/nginx/body", "/var/lib/nginx/proxy"] if self.method != "compose" else ["/var/cache/nginx/client_temp", "/var/cache/nginx/proxy_temp"]
        def sizes():
            output = self.execute(self.nginx, "du", "-sb", *directories).stdout
            return [int(line.split()[0]) for line in output.splitlines()]
        baseline = sizes()
        samples = []
        errors = []
        stop = threading.Event()
        def sample():
            while not stop.is_set():
                try:
                    samples.append(sizes())
                except Exception as error:
                    errors.append(error)
                    break
                stop.wait(0.1)
        thread = threading.Thread(target=sample)
        thread.start()
        try:
            self.drive("large")
        finally:
            stop.set()
            thread.join(timeout=30)
        require(not thread.is_alive() and not errors and bool(samples), "nginx sampling failed")
        samples.append(sizes())
        require(all(all(size <= before for size, before in zip(row, baseline)) for row in samples), "nginx temporary storage grew")
        self.say(f"PASS nginx_streams_without_temporary_growth baseline={baseline} maximum={[max(row[i] for row in samples) for i in range(len(baseline))]} samples={len(samples)}")

    def diagnostics(self):
        for name in self.containers:
            if self.docker("container", "inspect", name, check=False).returncode == 0:
                output = self.docker("logs", "--tail", "30", name, check=False)
                self.say(f"Diagnostics {name}:\n{output.stdout}\n{output.stderr}")
        if hasattr(self, "native"):
            result = self.execute(self.native, "journalctl", "-u", "lorebay", "-n", "30", "--no-pager", check=False)
            self.say(result.stdout)
        if self.compose_args:
            result = self.compose("logs", "--no-color", "--tail", "30", check=False)
            self.say(f"Compose diagnostics:\n{result.stdout}\n{result.stderr}")

    def audit(self, files=False):
        private = [p["password"].encode() for p in self.people]
        if token := self.environment.get("GH_TOKEN"):
            private.append(token.encode())
        for name in self.containers:
            if self.docker("container", "inspect", name, check=False).returncode:
                continue
            logs = self.docker("logs", name, check=False)
            raw = (logs.stdout + logs.stderr).encode()
            require(not any(secret in raw for secret in private), "Credential found in container logs: " + name)
        if hasattr(self, "native"):
            raw = self.execute(self.native, "journalctl", "-u", "lorebay", "--no-pager", check=False).stdout.encode()
            require(not any(secret in raw for secret in private), "Credential found in service journal")
        if self.compose_args:
            logs = self.compose("logs", "--no-color", check=False)
            require(logs.returncode == 0, "Cannot inspect Compose service logs")
            raw = (logs.stdout + logs.stderr).encode()
            require(not any(secret in raw for secret in private), "Credential found in Compose logs")
        if files:
            overlap = max(map(len, private))
            for path in self.root.rglob("*"):
                if not path.is_file() or path.is_symlink():
                    continue
                if path == self.root / "users.yaml":
                    raw = self.run("sudo", "-n", "cat", path).stdout.encode()
                    require(not any(secret in raw for secret in private), "Credential stored in users file")
                    continue
                with path.open("rb") as stream:
                    previous = b""
                    while chunk := stream.read(1024 * 1024):
                        raw = previous + chunk
                        require(not any(secret in raw for secret in private), "Credential stored in a suite file")
                        previous = raw[-overlap:]
        self.say("PASS logs_contain_no_github_token_or_test_password")

    def cleanup(self):
        failures = []
        if self.compose_args:
            result = self.compose("down", "--volumes", "--remove-orphans", check=False)
            if result.returncode:
                failures.append("Compose down: " + result.stderr)
        for name in reversed(self.containers):
            self.docker("rm", "-f", "-v", name, check=False)
        for name in reversed(self.networks):
            self.docker("network", "rm", name, check=False)
        for image in reversed(self.images):
            self.docker("image", "rm", image, check=False)
        if self.data_owned and self.data.exists():
            self.run("sudo", "-n", "rm", "-rf", "--", self.data)
        shutil.rmtree(self.root)
        for path in reversed(self.created_parents):
            if path.exists() and not any(path.iterdir()):
                path.rmdir()
        remaining = {}
        for kind in ("container", "network", "volume", "image"):
            result = self.docker(kind, "ls", "-q", "--filter", f"label={LABEL}={self.prefix}")
            remaining[kind] = result.stdout.split()
        remaining["images"] = [image for image in self.images if self.image_present(image)]
        remaining["paths"] = [str(path) for path in (self.root, self.data) if path.exists()]
        self.say("Cleanup listing: " + json.dumps(remaining, sort_keys=True))
        require(not failures and not any(remaining.values()), "Cleanup incomplete: " + "; ".join(failures))
        self.say("PASS cleanup_leaves_no_owned_resource")
        output = "\n".join(self.output)
        require(not any(re.search(re.escape(secret), output) for secret in self.secrets), "Credential output audit failed")
        self.say("PASS output_contains_no_github_token_or_test_password")

    def install_command(self, **changes):
        options = {
            "version": self.version, "domain": self.domain, "users": "/srv/inputs/users.yaml",
            "certificate": "/srv/inputs/certificate.pem", "key": "/srv/inputs/key.pem",
            "data": "/srv/data", "archives": "/srv/archives",
        } | changes
        return ["python3", "-B", "/srv/bundle/deploy/native/install.py", "install",
                *(part for key, value in options.items() for part in ("--" + key, str(value)))]

    def native_snapshot(self, name):
        return self.execute(name, "python3", "-c", "\n".join([
            "import hashlib,json,pwd", "from pathlib import Path", "items={}",
            "for base in ['/etc/lorebay','/opt/lorebay','/etc/nginx/lorebay-tls','/etc/systemd/system/lorebay.service','/etc/nginx/conf.d','/srv/data']:",
            " p=Path(base)",
            " for f in [p,*(sorted(p.rglob('*')) if p.is_dir() else [])]:",
            "  if f.exists() or f.is_symlink():",
            "   s=f.lstat();items[str(f)]=[s.st_mode,s.st_uid,s.st_gid,f.readlink().as_posix() if f.is_symlink() else hashlib.sha256(f.read_bytes()).hexdigest() if f.is_file() else None]",
            "items['user']=[p.pw_name for p in pwd.getpwall() if p.pw_name=='lorebay']", "print(json.dumps(items,sort_keys=True))",
        ])).stdout

    def refusal(self, name, message, **changes):
        before = self.native_snapshot(name)
        result = self.execute(name, *self.install_command(**changes), check=False)
        require(result.returncode != 0 and message in result.stdout + result.stderr,
                "Installer did not refuse correctly: " + result.stdout + result.stderr)
        require(self.native_snapshot(name) == before, "Refused install changed managed state")
        self.say("PASS native_install_refuses " + message)

    def native_inputs(self, name):
        self.execute(name, "mkdir", "-p", "/srv/inputs", "/srv/archives", "/srv/bundle", "/srv/bin")
        self.copy(str(self.bundle) + "/.", name, "/srv/bundle/")
        for filename in ("SHA256SUMS", "lorebayd-x86_64-unknown-linux-musl.tar.gz"):
            self.copy(self.release / filename, name, "/srv/archives/" + filename)
        for key, filename in (("certificate.pem", "certificate.pem"), ("key.pem", "key.pem"),
                              ("authority.pem", "authority.pem"), ("users.yaml", "users.yaml"),
                              ("replacement.pem", "replacement.pem"), ("replacement.key", "replacement.key"),
                              ("other.pem", "other.pem"), ("other.key", "other.key")):
            self.copy(self.root / key, name, "/srv/inputs/" + filename)
        self.copy(self.root / "lorebayd/lorebayd", name, "/srv/bin/lorebayd")
        self.execute(name, "chown", "-R", "root:root", "/srv/inputs", "/srv/archives", "/srv/bundle", "/srv/bin")
        self.execute(name, "chmod", "0700", "/srv/inputs")
        self.execute(name, "sh", "-c", "chmod 0600 /srv/inputs/*; chmod 0644 /srv/archives/*; chmod 0755 /srv/archives /srv/bundle /srv/bin")

    def native_checks(self, base):
        self.method = "native-" + base.split(":")[0]
        self.stage = self.method + ": install"
        self.say("BEGIN " + self.stage)
        network = self.network(self.method)
        self.choose_port()
        name = self.container(self.method, self.server_images[base], "--network", network,
                              "--privileged", "--cgroupns=private", "--security-opt", "label=disable",
                              "--tmpfs", "/run", "--tmpfs", "/run/lock", "--tmpfs", "/tmp:exec",
                              "--add-host", f"{self.domain}:127.0.0.1", "-p", f"127.0.0.1:{self.port}:443")
        self.native = self.nginx = name
        for _ in range(60):
            status = self.execute(name, "systemctl", "is-system-running", check=False).stdout.strip()
            if status in ("running", "degraded"):
                break
            time.sleep(1)
        require(status in ("running", "degraded"), "systemd did not boot")
        self.native_inputs(name)
        if base == "debian:12":
            self.execute(name, "apt-get", "update")
            self.execute(name, "apt-get", "install", "-y", "--no-upgrade", "nginx")
            other = (
                "server { listen 443 ssl; listen [::]:443 ssl; server_name other.example.com; "
                "ssl_certificate /srv/inputs/other.pem; ssl_certificate_key /srv/inputs/other.key; "
                "location / { return 200 'other site'; } }\n"
            )
            self.execute(name, "sh", "-c", "cat > /etc/nginx/conf.d/other.conf", input=other)
            self.execute(name, "systemctl", "restart", "nginx")
        self.refusal(name, "Required input file is missing", users="/srv/inputs/missing.yaml")
        self.refusal(name, "outside /tmp and /var/tmp", data="/tmp/refused-data")
        self.execute(name, "mkdir", "-m", "0777", "/srv/open")
        self.refusal(name, "parent must be owned by root and not writable", data="/srv/open/data")
        config = {"data_directory": "/srv/other-data", "users_file": "/srv/inputs/users.yaml",
                  "listen_address": "127.0.0.1:18080", "public_url": "http://127.0.0.1:18080"}
        self.execute(name, "sh", "-c", "cat > /srv/other.json", input=json.dumps(config))
        self.execute(name, "sh", "-c", "/srv/bin/lorebayd --config /srv/other.json > /srv/other.log 2>&1 & echo $! > /srv/other.pid")
        for _ in range(40):
            result = self.execute(name, "curl", "-fsS", "http://127.0.0.1:18080/readyz", check=False)
            if result.returncode == 0:
                break
            time.sleep(0.5)
        require(result.returncode == 0, "Unmanaged database fixture did not start")
        self.execute(name, "sh", "-c", "kill -TERM $(cat /srv/other.pid); while kill -0 $(cat /srv/other.pid) 2>/dev/null && ! grep -q '^State:.*Z' /proc/$(cat /srv/other.pid)/status; do sleep 0.1; done")
        self.refusal(name, "Data directory is not empty", data="/srv/other-data")
        self.execute(name, "cp", "-a", "/srv/archives", "/srv/tampered")
        self.execute(name, "python3", "-c", "from pathlib import Path;p=Path('/srv/tampered/SHA256SUMS');s=p.read_text();p.write_text('\\n'.join('0'*64+line[64:] if line.endswith('  lorebayd-x86_64-unknown-linux-musl.tar.gz') else line for line in s.splitlines())+'\\n')")
        self.refusal(name, "checksum mismatch or missing archive entry", archives="/srv/tampered")
        installed = self.execute(name, *self.install_command())
        self.say(installed.stdout.strip())
        self.ready()
        require("nothing changed" in self.execute(name, *self.install_command()).stdout, "Identical install was not idempotent")
        self.refusal(name, "Another version is installed", version="v0.0.0")
        require(self.execute(name, "stat", "-c", "%U:%G %a", "/srv/data").stdout.strip() == "lorebay:lorebay 700", "Native data ownership is not private")
        require(self.execute(name, "stat", "-c", "%a", "/srv/data/lorebay.sqlite3").stdout.strip() == "600", "Native database mode is not private")
        listeners = self.execute(name, "ss", "-ltnp").stdout
        app = [line for line in listeners.splitlines() if "lorebayd" in line]
        require(len(app) == 1 and "127.0.0.1:8080" in app[0], "Native service is not loopback-only")
        self.say("PASS native_install_serves_https_with_private_modes_and_loopback_listener " + base)
        self.clients(network, name)
        self.operation_checks()
        self.certificate_checks()
        self.stage = self.method + ": persistence"
        self.execute(name, "systemctl", "restart", "lorebay")
        self.ready()
        self.drive("persist")
        self.docker("restart", name)
        self.ready()
        self.drive("persist")
        self.drive("revoke")
        self.loopback_checks(name, network)
        self.stage = self.method + ": removal"
        self.execute(name, "systemctl", "stop", "lorebay")
        inputs = self.execute(name, "sh", "-c", "sha256sum /srv/inputs/*").stdout
        before = self.execute(name, "sh", "-c", "find /srv/data -type f -exec sha256sum {} + | sort").stdout
        self.execute(name, "curl", "-fsS", "http://127.0.0.1/")
        if base == "debian:12":
            self.other_site(name)
        self.execute(name, "python3", "-B", "/srv/bundle/deploy/native/install.py", "uninstall")
        after = self.execute(name, "sh", "-c", "find /srv/data -type f -exec sha256sum {} + | sort").stdout
        require(before == after, "Uninstall changed kept data")
        require(inputs == self.execute(name, "sh", "-c", "sha256sum /srv/inputs/*").stdout, "Uninstall changed supplied inputs")
        self.execute(name, "sh", "-c", "test ! -e /etc/lorebay && test ! -e /opt/lorebay && ! getent passwd lorebay && test $(stat -c %U /srv/data) = root")
        self.execute(name, "curl", "-fsS", "http://127.0.0.1/")
        if base == "debian:12":
            self.other_site(name)
        self.say("PASS native_uninstall_keeps_data_inputs_and_other_nginx_sites " + base)
        self.audit()
        for container in (self.client_a, self.client_b, name):
            self.docker("rm", "-f", "-v", container)
        del self.native

    def other_site(self, name):
        result = self.execute(name, "curl", "-fsS", "--cacert", "/srv/inputs/other.pem", "--resolve",
                              "other.example.com:443:127.0.0.1", "https://other.example.com/")
        require(result.stdout == "other site", "Other nginx site stopped serving")

    def unsupported(self):
        self.stage = "unsupported native host"
        name = self.container("unsupported", "debian:13", command=["sleep", "infinity"])
        self.execute(name, "apt-get", "update")
        self.execute(name, "apt-get", "install", "-y", "python3")
        self.native_inputs(name)
        self.refusal(name, "Supported hosts: x86_64 Debian 12 or Ubuntu 24.04 only")
        self.docker("rm", "-f", "-v", name)

    def certificate_checks(self):
        self.stage = self.method + ": certificates"
        if self.method == "compose":
            command = ["python3", "-B", self.bundle / "deploy/common/replace-certificate.py",
                       "--domain", self.domain, "--directory", self.root / "tls", "--container", self.nginx,
                       "--connect", self.address(self.nginx, self.compose_network)]
            def replace(certificate, key, check=True):
                return self.run(*command, "--certificate", self.root / certificate, "--key", self.root / key, check=check)
            def restart():
                self.compose("restart", "nginx")
                self.ready()
            def reload():
                self.execute(self.nginx, "nginx", "-s", "reload")
        else:
            def replace(certificate, key, check=True):
                return self.execute(self.native, "python3", "-B", "/opt/lorebay/deploy/common/replace-certificate.py",
                                    "--domain", self.domain, "--certificate", "/srv/inputs/" + certificate,
                                    "--key", "/srv/inputs/" + key, check=check)
            def restart():
                self.execute(self.native, "systemctl", "restart", "nginx")
                self.ready()
            def reload():
                self.execute(self.native, "systemctl", "reload", "nginx")
        original = self.fingerprint()
        for certificate, key, diagnostic in (("other.pem", "other.key", "domain"), ("certificate.pem", "other.key", "key")):
            result = replace(certificate, key, check=False)
            require(result.returncode != 0 and diagnostic in (result.stdout + result.stderr).lower(), "Invalid certificate was not rejected")
            require(self.fingerprint() == original, "Rejected certificate changed live TLS")
            reload()
            require(self.fingerprint() == original, "Rejected pair replaced TLS after reload")
            restart()
            require(self.fingerprint() == original, "Rejected pair replaced TLS after restart")
            self.say("PASS certificate_rejection_keeps_live_pair_after_reload_and_restart " + diagnostic)
        replace("replacement.pem", "replacement.key")
        replacement = self.fingerprint(self.root / "replacement.pem")
        expected = hashlib.sha256(ssl.PEM_cert_to_DER_cert((self.root / "replacement.pem").read_text())).hexdigest()
        require(replacement == expected and replacement != original, "Replacement certificate was not served")
        replace("certificate.pem", "key.pem")
        self.ready()
        require(self.fingerprint() == original, "Supplied certificate was not restored")
        self.say("PASS certificate_replacement_serves_new_pair_and_restores_supplied_pair")

    def loopback_checks(self, name, network):
        self.stage = self.method + ": loopback sign-in"
        self.execute(self.client_a, "ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", "/work/ssh-key")
        public = self.execute(self.client_a, "cat", "/work/ssh-key.pub").stdout
        self.execute(name, "mkdir", "-m", "0700", "-p", "/root/.ssh")
        self.execute(name, "sh", "-c", "cat > /root/.ssh/authorized_keys; chmod 0600 /root/.ssh/authorized_keys", input=public)
        self.execute(name, "systemctl", "start", "ssh")
        original = self.execute(name, "cat", "/etc/lorebay/lorebayd.json").stdout
        config = json.loads(original)
        try:
            for loopback in (True, False):
                changed = config | {"public_url": "http://127.0.0.1:8080" if loopback else self.origin}
                self.execute(name, "sh", "-c", "cat > /etc/lorebay/lorebayd.json", input=json.dumps(changed))
                self.execute(name, "systemctl", "restart", "lorebay")
                self.ready()
                self.drive("loopback", server=self.address(name, network), loopback_origin=loopback)
        finally:
            self.execute(name, "sh", "-c", "cat > /etc/lorebay/lorebayd.json", input=original)
            self.execute(name, "systemctl", "restart", "lorebay")
            self.ready()
        require(self.execute(name, "cat", "/etc/lorebay/lorebayd.json").stdout == original, "Loopback check changed the original configuration")

    def compose_checks(self):
        self.method = "compose"
        self.stage = "compose: start and refusals"
        self.say("BEGIN " + self.stage)
        self.choose_port()
        data = self.data / "data"
        self.run("sudo", "-n", "mkdir", "-m", "0700", data)
        self.run("sudo", "-n", "chown", "65532:65532", data, self.root / "users.yaml")
        self.run("sudo", "-n", "chmod", "0400", self.root / "users.yaml")
        access = self.container(
            "compose-access", self.client_image, "--network", "none", "--user", "65532:65532",
            "--mount", f"type=bind,source={self.root}/users.yaml,target=/inputs/users.yaml,readonly",
            "--mount", f"type=bind,source={data},target=/data",
            command=["sh", "-c", "test -r /inputs/users.yaml && test -w /data"],
        )
        require(self.docker("wait", access).stdout.strip() == "0", "Service UID cannot access the bind-mounted inputs")
        self.docker("rm", "-v", access)
        self.say("PASS compose_bind_mounts_allow_service_access_with_private_ancestors")
        tls = self.root / "tls"
        (tls / "initial").mkdir(parents=True)
        tls.chmod(0o700)
        for name in ("certificate.pem", "key.pem"):
            shutil.copyfile(self.root / name, tls / "initial" / name)
            (tls / "initial" / name).chmod(0o600)
        (tls / "live").symlink_to("initial")
        env = self.root / "compose.env"
        env.write_text(
            f"DOMAIN={self.domain}\nHTTPS_PORT=127.0.0.1:{self.port}\nHTTPS_SUFFIX=\n"
            f"DATA_DIRECTORY={data}\nUSERS_FILE={self.root}/users.yaml\n"
            f"CONFIG_FILE={self.bundle}/deploy/docker/lorebayd.json\nCERTIFICATE_DIRECTORY={tls}\n"
        )
        override = self.root / "compose.json"
        override.write_text(json.dumps({
            "services": {"lorebayd": {"image": self.server_image, "pull_policy": "never", "labels": {LABEL: self.prefix}},
                         "nginx": {"labels": {LABEL: self.prefix}}},
            "networks": {"default": {"labels": {LABEL: self.prefix}}},
        }))
        self.compose_args = ["docker", "compose", "--project-name", self.prefix, "--env-file", str(env),
                             "-f", str(self.bundle / "deploy/docker/compose.yml"), "-f", str(override)]
        details = json.loads(self.compose("config", "--format", "json").stdout)
        self.pull(details["services"]["nginx"]["image"])
        for variable in ("DATA_DIRECTORY", "USERS_FILE", "CONFIG_FILE", "CERTIFICATE_DIRECTORY"):
            missing = self.root / ("missing-" + variable.lower())
            result = self.compose("up", "-d", env=self.environment | {variable: str(missing)}, check=False)
            require(result.returncode != 0 and not missing.exists() and "does not exist" in result.stderr, "Missing bind source was not refused: " + result.stderr)
            self.audit()
            self.compose("down", "--volumes", "--remove-orphans")
            self.say("PASS compose_refuses_missing_bind_source " + variable)
        refusal = self.root / "refusal.json"
        refusal.write_text(json.dumps({"services": {"lorebayd": {"restart": "no"}}}))
        self.run("sudo", "-n", "chmod", "0000", self.root / "users.yaml")
        try:
            self.run(*self.compose_args, "-f", refusal, "up", "-d", "lorebayd")
            app = self.compose("ps", "-a", "-q", "lorebayd").stdout.strip()
            for _ in range(30):
                status = json.loads(self.docker("inspect", app).stdout)[0]["State"]
                if not status["Running"]:
                    break
                time.sleep(1)
            require(not status["Running"] and status["ExitCode"] != 0, "Unreadable users file did not stop the server")
            logs = self.compose("logs", "--no-color", "lorebayd").stdout
            require("/etc/lorebay/users.yaml" in logs, "Unreadable users diagnostic omits the input")
            self.say("PASS compose_refuses_unreadable_users_file")
        finally:
            try:
                self.audit()
            finally:
                self.run("sudo", "-n", "chmod", "0400", self.root / "users.yaml")
                self.compose("down", "--volumes", "--remove-orphans")
        self.compose("up", "-d", "--wait", "--wait-timeout", "120")
        self.ready()
        app = self.compose("ps", "-q", "lorebayd").stdout.strip()
        self.nginx = self.compose("ps", "-q", "nginx").stdout.strip()
        inspect = json.loads(self.docker("inspect", app).stdout)[0]
        require(not inspect["HostConfig"]["PortBindings"] and inspect["Config"]["User"] == "65532:65532", "Application has a host port or incorrect user")
        self.compose_network = next(iter(inspect["NetworkSettings"]["Networks"]))
        self.networks.append(self.compose_network)
        require(self.run("sudo", "-n", "stat", "-c", "%u:%g %a", data).stdout.strip() == "65532:65532 700", "Compose data is not private")
        require(self.run("sudo", "-n", "stat", "-c", "%a", data / "lorebay.sqlite3").stdout.strip() == "600", "Compose database is not private")
        publications = json.loads(self.docker("inspect", self.nginx).stdout)[0]["HostConfig"]["PortBindings"]
        require(publications == {"443/tcp": [{"HostIp": "127.0.0.1", "HostPort": str(self.port)}]}, "nginx publication is not loopback-only")
        self.say("PASS compose_serves_https_with_private_modes_and_no_application_host_port")
        self.clients(self.compose_network, self.nginx)
        self.operation_checks()
        self.certificate_checks()
        self.stage = "compose: persistence"
        for operation in (("restart",), ("up", "-d", "--force-recreate", "--wait", "--wait-timeout", "120"), ("down",)):
            self.audit()
            self.compose(*operation)
            if operation == ("down",):
                self.compose("up", "-d", "--wait", "--wait-timeout", "120")
            self.ready()
            self.nginx = self.compose("ps", "-q", "nginx").stdout.strip()
            address = self.address(self.nginx, self.compose_network)
            for client in (self.client_a, self.client_b):
                script = "import sys;from pathlib import Path;p=Path('/etc/hosts');p.write_text('\\n'.join(line for line in p.read_text().splitlines() if sys.argv[1] not in line.split()[1:])+'\\n'+sys.argv[2]+' '+sys.argv[1]+'\\n')"
                self.execute(client, "python3", "-c", script, self.domain, address)
            self.drive("persist")
        self.drive("revoke")
        self.stage = "compose: removal"
        self.compose("stop")
        self.audit()
        before = self.run("sudo", "-n", "find", data, "-type", "f", "-exec", "sha256sum", "{}", "+").stdout
        self.compose("down", "--volumes", "--remove-orphans")
        after = self.run("sudo", "-n", "find", data, "-type", "f", "-exec", "sha256sum", "{}", "+").stdout
        require(sorted(before.splitlines()) == sorted(after.splitlines()) and bool(after.strip()), "Compose removal changed kept data")
        self.say("PASS compose_removal_keeps_data_bytes")
        for name in (self.client_a, self.client_b):
            self.docker("rm", "-f", "-v", name)

    def archive_check(self):
        self.stage = "archive recipe"
        name = self.container("archive", "alpine:3", command=["sleep", "infinity"])
        self.execute(name, "apk", "add", "--no-cache", "curl")
        self.execute(name, "mkdir", "-p", "/work")
        for filename in ("SHA256SUMS", "lorebay-x86_64-unknown-linux-musl.tar.gz", "lorebayd-x86_64-unknown-linux-musl.tar.gz"):
            self.copy(self.release / filename, name, "/work/" + filename)
        self.run("sudo", "-n", "chmod", "0404", self.root / "users.yaml")
        try:
            self.copy(self.root / "users.yaml", name, "/work/users.yaml")
        finally:
            self.run("sudo", "-n", "chmod", "0400", self.root / "users.yaml")
        self.execute(name, "chmod", "0600", "/work/users.yaml")
        self.execute(name, "sh", "-c", "cd /work && grep -E '  lorebayd?-x86_64-unknown-linux-musl\\.tar\\.gz$' SHA256SUMS | sha256sum -c - && tar -xzf lorebay-x86_64-unknown-linux-musl.tar.gz && install -m 0755 lorebay /usr/local/bin/ && tar -xzf lorebayd-x86_64-unknown-linux-musl.tar.gz")
        for binary in ("/usr/local/bin/lorebay", "/work/lorebayd"):
            require(self.execute(name, binary, "--version").stdout.strip().endswith(self.version[1:]), "Alpine archive version differs")
        self.execute(name, "/work/lorebayd", "hash-password", input=secrets.token_urlsafe(24) + "\n")
        config = {"data_directory": "/work/data", "users_file": "/work/users.yaml",
                  "listen_address": "127.0.0.1:8080", "public_url": "http://127.0.0.1:8080"}
        self.execute(name, "sh", "-c", "cat > /work/config.json", input=json.dumps(config))
        self.execute(name, "sh", "-c", "/work/lorebayd --config /work/config.json > /work/server.log 2>&1 &")
        for _ in range(30):
            result = self.execute(name, "/work/lorebayd", "readiness", "--config", "/work/config.json", check=False)
            if result.returncode == 0:
                break
            time.sleep(1)
        require(result.returncode == 0, "Alpine archive readiness failed")
        result = self.execute(name, "curl", "-fsS", "http://127.0.0.1:8080/readyz")
        require(json.loads(result.stdout)["data"] == "ok", "Alpine HTTP readiness failed")
        self.say("PASS archive_runs_on_alpine_x86_64 manual executables, loopback HTTP; no nginx or service-manager qualification")
        self.docker("rm", "-f", "-v", name)


def main():
    if len(sys.argv) != 2:
        print("Usage: python3 -B tests/deploy/run.py CONFIG.json", file=sys.stderr)
        return 2
    started = time.monotonic()
    suite = None
    failed = False
    try:
        suite = Suite(Path(sys.argv[1]).expanduser().resolve(strict=True))
        suite.say("BEGIN deployment suite " + suite.prefix)
        suite.release_files()
        suite.prepare()
        suite.unsupported()
        for base in ("debian:12", "ubuntu:24.04"):
            suite.native_checks(base)
        suite.compose_checks()
        suite.archive_check()
    except (Exception, KeyboardInterrupt) as error:
        failed = True
        if suite:
            suite.say(f"FAIL stage={suite.stage}: {type(error).__name__}: {error}")
            try:
                suite.diagnostics()
            except Exception as diagnostic:
                suite.say("Diagnostics unavailable: " + str(diagnostic))
        else:
            print("FAIL configuration: " + str(error), file=sys.stderr)
    finally:
        if suite:
            try:
                suite.audit(files=True)
            except Exception as error:
                failed = True
                suite.say("FAIL credential audit: " + str(error))
            try:
                suite.cleanup()
            except Exception as error:
                failed = True
                suite.say("FAIL cleanup: " + str(error))
            suite.say(f"SUMMARY {'FAILED' if failed else 'PASSED'} duration={time.monotonic() - started:.1f}s")
    return 1 if failed else 0


if __name__ == "__main__":
    def interrupted(*_args):
        raise KeyboardInterrupt("Interrupted")

    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main())
