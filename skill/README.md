# Lorebay skill

Lorebay's agent skill, in `lorebay/`, tells a coding agent how to record its
work in a Lorebay space. A person installs it in Claude Code 2.1.328 or Codex
0.170.0, or newer, and connects the harness to a `lorebayd`, whose address the
person deploying it gives. Older harness versions are untested.

## Usage

Install the skill as a plugin from this repo's marketplace at the server's
release tag, such as `v0.1.0`, or at `main` for a server built from `main`.
Connect the harness at user level, so every project and worktree has the server,
under the name `lorebay` that the skill uses. `<url>` is `https://` with the
host and port, such as `https://lorebay.test:8443`; both harnesses refuse OAuth
over `http://` except on `localhost`, `127.0.0.1` or `::1`. For Claude Code, in
a terminal, since `mcp login` needs one:

```bash
claude plugin marketplace add https://github.com/lorebayai/lorebay.git#v0.1.0
claude plugin install lorebay@lorebay
claude mcp add --scope user --transport http lorebay <url>/mcp
claude mcp login lorebay
```

For Codex, `mcp add` also starts the sign-in when it reaches the server, and
`mcp login` repeats it. Then edit `~/.codex/config.toml` so it holds the lines
below: `sandbox_mode` above the first `[table]`, and the approval line in the
`[mcp_servers.lorebay]` table that `mcp add` wrote. The sandbox lines let the
agent's shell reach the network, in every project, to upload files; the approval
lets non-interactive runs, such as `codex exec`, call the tools.

```bash
codex plugin marketplace add https://github.com/lorebayai/lorebay.git --ref v0.1.0
codex plugin add lorebay@lorebay
codex mcp add lorebay --url <url>/mcp
```

```toml
sandbox_mode = "workspace-write"
[sandbox_workspace_write]
network_access = true
[mcp_servers.lorebay]
url = "<url>/mcp"
default_tools_approval_mode = "approve"
```

A new session loads the skill: `claude plugin details lorebay@lorebay` and
`codex plugin list` show the plugin, and Claude Code's `/skills` selector lists
`lorebay:lorebay`. To update, run `claude plugin marketplace update lorebay` and
`claude plugin update lorebay@lorebay`, or turn on auto-update in `/plugin`; for
Codex, run `codex plugin marketplace upgrade lorebay`. Both refetch the ref the
marketplace was added at; when the server moves to another release, run
`claude plugin marketplace remove lorebay` or
`codex plugin marketplace remove lorebay`, then add it at the new tag and
install the plugin as above. Remove an older install's links,
`~/.claude/skills/lorebay` and `~/.agents/skills/lorebay`, or the skill loads
twice. `--no-browser` on either `mcp login` prints the authorization link only.
On Lorebay's authorization page, sign in, consent and name the agent, such as
`Claude Code (home)`. Authorizing the same harness again suggests the same name:
pick another, or replace the old authorization and keep the name.
`lorebay agent revoke '<name>'` ends one. When the harness says `lorebay` needs
authentication, run `mcp login lorebay` again: until then no work is recorded,
and the agent says so only if the harness loads the skill, which Codex was seen
to skip in that state. When more than 30 days, the server's default, pass
between `claude mcp add` and the first authorization, the server has dropped the
unused client registration: run `claude mcp remove --scope user lorebay` and add
the server again.

A server whose certificate comes from a private certificate authority, such as
`https://lorebay.test:8443`, needs on each machine that connects: a hosts line
`<server address> lorebay.test`, also for the browser that opens the
authorization page, which warns unless it trusts the authority; the authority's
certificate at a fixed path, such as `~/.local/share/lorebay/ca.crt`, not in the
system trust store; and every process that connects trusting it, as
Configuration lists. The authority's key never leaves its machine. `bundle.pem`
joins the certificate to the system's authorities, which `SSL_CERT_FILE`
replaces; on Debian or Ubuntu, with `ca-certificates` installed:

```bash
cd ~/.local/share/lorebay && cat /etc/ssl/certs/ca-certificates.crt ca.crt > bundle.pem
```

A person in several spaces names one per project, and the agent writes a line
such as ``- Lorebay space: `SPACE-1` `` into the project's committed agent
instructions files. The skill sends the person to `lorebay` commands, such as
`lorebay space create` and `lorebay task delete`. Get the CLI and sign in with
`lorebay login <url>`, as the [root README](../README.md#getting-started) shows;
[cli/README.md](../cli/README.md) lists its commands.

## Configuration

| Environment variable | Set for | Value |
| --- | --- | --- |
| `NODE_EXTRA_CA_CERTS` | Claude Code, on every run or in `env` of `~/.claude/settings.json` | Absolute path of `ca.crt` |
| `CODEX_CA_CERTIFICATE` | Codex, on every run | Absolute path of `ca.crt` |
| `SSL_CERT_FILE` | Both harnesses, for uploads from the agent's shell, and the `lorebay` CLI | Absolute path of `bundle.pem` |

## Docs

See Lorebay's [Agent skill](../docs/contracts/agent-skill.md) contract.
