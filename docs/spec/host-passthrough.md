# Host Passthrough

For host passthrough rules, the launch repository is the resolved canonical git root for `run`, `start`, and `exec`, and the selected managed session's stored canonical git root for `restart`.

## Host Git Identity Passthrough

For `run`, `start`, and `restart`, `agentbox` passes the launch repository's effective host Git identity into the runtime container so Git operations inside the agent environment use the same default author identity as the host repository. `agentbox exec` is a Codex-only one-shot mode and uses a fixed Codex author identity instead.

Rules:

- For `run`, `start`, and `restart`, `agentbox` reads only the effective `user.name` and `user.email` Git config values from the launch repository during launch preparation.
- For `run`, `start`, and `restart`, if either value is unset, that value is not injected.
- For `run`, `start`, and `restart`, if reading either value fails unexpectedly, `agentbox` prints a warning and continues launching without that value.
- For `exec`, `agentbox` injects `user.name=Codex` and `user.email=noreply@openai.com` instead of reading those values from the launch repository.
- Present values are injected with Git's `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_*`, and `GIT_CONFIG_VALUE_*` environment variables.
- For `run`, `start`, `restart`, and `exec`, present Git identity values are also materialized during runtime startup into the runtime home's Git config at `$XDG_CONFIG_HOME/git/config`, falling back to `$HOME/.config/git/config` when `XDG_CONFIG_HOME` is unset or empty.
- Runtime Git identity materialization uses an `agentbox`-managed include file under the runtime Git config directory. `agentbox` refreshes the managed identity file when an identity is present and removes it when no identity is present, so persisted runtime home volumes do not retain stale managed identity values.
- Runtime Git identity materialization preserves unrelated runtime Git config settings and ensures the managed identity include is not duplicated.
- `agentbox` does not modify the workspace repository's `.git/config`.
- Git identity passthrough does not depend on `SSH_AUTH_SOCK`.
- `agentbox` does not mount the host Git config files, credential helpers, or other Git configuration for identity passthrough.

## Host Git Excludes File Passthrough

For `run`, `start`, `restart`, and `exec`, `agentbox` passes the launch repository's effective host Git excludes file into the runtime container when that file exists. This makes container Git commands honor the same host ignore patterns without mounting host Git configuration files.

Rules:

- `agentbox` first reads the launch repository's effective `core.excludesFile` value with `git -C <git-root> config --path --get core.excludesFile`.
- If `core.excludesFile` is unset, `agentbox` uses Git's default excludes file path: `${XDG_CONFIG_HOME}/git/ignore` when `XDG_CONFIG_HOME` is set and non-empty, otherwise `${HOME}/.config/git/ignore`.
- If no source path can be determined, or if the source file does not exist, container launch behavior is unchanged.
- If the source path exists and is a readable regular file, `agentbox` bind-mounts it read-only at `/run/agentbox/git-ignore` and injects `core.excludesFile=/run/agentbox/git-ignore` with Git's `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_*`, and `GIT_CONFIG_VALUE_*` environment variables.
- If the configured source path is relative, `agentbox` resolves it relative to the canonical git root.
- If reading `core.excludesFile` fails unexpectedly, the source path is not UTF-8, or the resolved source exists but is not a readable regular file, `agentbox` prints a warning, skips the excludes passthrough, and continues launching.
- Git excludes file passthrough does not depend on `SSH_AUTH_SOCK`.
- `agentbox` does not mount the host Git config files, credential helpers, `~/.gitconfig`, or the full `${XDG_CONFIG_HOME}/git` directory for excludes passthrough.

## SSH Commit Signing Passthrough

When the invoking host environment has a usable SSH agent socket, `run`, `start`, `restart`, and `exec` make SSH-based Git commit signing available inside the runtime container without mounting private keys or host SSH configuration.

Rules:

- `agentbox` detects `SSH_AUTH_SOCK` on the host during launch preparation.
- If `SSH_AUTH_SOCK` is unset, container launch behavior is unchanged.
- If `SSH_AUTH_SOCK` is set but does not point to an accessible Unix socket, `agentbox` prints a warning, does not mount it, and continues launching the container.
- If `SSH_AUTH_SOCK` points to an accessible Unix socket, `agentbox` bind-mounts that socket at `/run/agentbox/ssh-agent.sock` and sets `SSH_AUTH_SOCK=/run/agentbox/ssh-agent.sock` inside the container.
- `agentbox` reads only the additional effective Git config values needed for SSH commit signing from the launch repository: `gpg.format`, `user.signingkey`, and `commit.gpgsign`.
- Those Git config values are injected alongside host Git identity passthrough values with Git's `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_*`, and `GIT_CONFIG_VALUE_*` environment variables.
- Signing-specific Git config values `gpg.format`, `user.signingkey`, and `commit.gpgsign` are injected only when the effective host `gpg.format` is `ssh`; GPG signing configuration is not passed through.
- `agentbox` does not mount the host Git config files, credential helpers, `~/.ssh`, private keys, or GPG agent sockets for commit signing passthrough.
- If `user.signingkey` is an SSH public key literal, `agentbox` passes that literal unchanged.
- If `user.signingkey` is a public key file path, `agentbox` reads the public key file and passes the key literal instead of the path.
- If `user.signingkey` is a private key path, `agentbox` does not read the private key. If a sibling `<path>.pub` file exists and is readable, `agentbox` reads that public key and passes the key literal instead.
- `agentbox` does not verify that the configured signing key is currently loaded in the SSH agent. Git and ssh-agent own the final signing error if the agent cannot sign.
- GPG commit signing passthrough is unsupported.

## Git SSH Remote Host Verification

When `run`, `start`, `restart`, or `exec` launches a container with a usable host SSH agent socket, `agentbox` also prepares SSH host-key verification for Git SSH remotes in the launch repository. This supports SSH remote authentication without mounting the host user's SSH directory, private keys, full Git configuration, or complete known_hosts files into the container.

The user may provide additional trusted host-key lines through the `knownHosts` field in the user config described in [Overview](overview.md#supported-environment-and-limits).

Rules:

- User config `knownHosts` entries are treated as trusted single-line known-host entries for the launch; they do not modify the host user's known_hosts files.
- `agentbox` detects SSH Git remote hosts from the launch repository's configured remotes. SCP-like URLs such as `git@github.com:owner/repo.git`, `ssh://` URLs, and `git+ssh://` URLs are considered SSH remotes. HTTPS remotes and local paths are ignored.
- For each detected SSH remote host, `agentbox` runs `ssh -G <host>` to resolve the host's OpenSSH configuration. Host known_hosts lookup uses the resolved `hostname`, `port`, `hostkeyalias`, `userknownhostsfile`, and `globalknownhostsfile` values.
- If `ssh -G <host>` is unavailable or fails, host known_hosts lookup falls back to the detected SSH remote host and the invoking user's `$HOME/.ssh/known_hosts` and `$HOME/.ssh/known_hosts2` files.
- Host known_hosts lookup uses `ssh-keygen -F <host> -f <file>` so hashed known_hosts entries can match. Missing `ssh-keygen`, missing known_hosts files, no match, or unexpected lookup failures are non-fatal.
- If no known_hosts entry is found for a detected SSH remote host, `agentbox` prints a warning naming that host and continues launching.
- `agentbox` combines exact lines returned from matching host known_hosts lookups with config `knownHosts` entries, deduplicates exact duplicate lines, and writes the result to a temporary host file for the launch.
- If there is at least one known_hosts line, that temporary file is bind-mounted read-only at `/run/agentbox/known_hosts`, and the container receives `GIT_SSH_COMMAND=ssh -o UserKnownHostsFile=/run/agentbox/known_hosts -o StrictHostKeyChecking=yes`.
- The mounted `/run/agentbox/known_hosts` file contains the combined known_hosts lines for the lifetime of the container process that receives `GIT_SSH_COMMAND`.
- `agentbox` does not embed public keys for hosted Git providers, run `ssh-keyscan`, automatically trust remote hosts on first use, write known_hosts data to agentbox state, write known_hosts data under `/home/user`, modify the host user's known_hosts files, or mount the host user's `~/.ssh` directory by default.

## Codex Host Configuration Passthrough

Codex server sessions use the invoking host user's Codex configuration directory as the runtime Codex home. A non-empty `CODEX_HOME` overrides the default host Codex home for server sessions.

Rules:

- For `agentbox run --runtime codex`, `agentbox start --runtime codex`, and `agentbox restart` of a Codex session, if `CODEX_HOME` is set to a non-empty value, that host directory is bind-mounted read-write at the same absolute path inside the container and the server container receives `CODEX_HOME` with the same value.
- If `CODEX_HOME` is unset or empty for those Codex server launches, the host `${HOME}/.codex` directory is bind-mounted read-write at `/home/user/.codex`.
- The mount is required so auth refreshes, skills, MCP configuration, plugins, rules, and other Codex user state remain consistent between host and container Codex clients.
- `run --runtime codex`, `start --runtime codex`, and `restart` of a Codex session fail before starting or replacing a container if the selected host Codex home directory is missing, is not a directory, is not an absolute path, or is not readable and writable by the invoking host user.
- `agentbox exec` uses the default host `${HOME}/.codex` passthrough at `/home/user/.codex` and does not apply `CODEX_HOME` server passthrough.
- `agentbox` does not create, migrate, or write files inside the selected host Codex home directory.
- OpenCode sessions do not receive the Codex passthrough mount.

## OpenCode Host State Passthrough

OpenCode sessions use the invoking host user's OpenCode configuration and data directories as the runtime OpenCode state.

Rules:

- For `agentbox run --runtime opencode`, `agentbox start --runtime opencode`, and `agentbox restart` of an OpenCode session, the host `${XDG_CONFIG_HOME:-$HOME/.config}/opencode` directory is bind-mounted read-write at `/home/user/.config/opencode`.
- For `agentbox run --runtime opencode`, `agentbox start --runtime opencode`, and `agentbox restart` of an OpenCode session, the host `${XDG_DATA_HOME:-$HOME/.local/share}/opencode` directory is bind-mounted read-write at `/home/user/.local/share/opencode`.
- Both host directories are required so global configuration, provider settings, authentication state, and other OpenCode user state remain consistent between host and container OpenCode clients.
- `run --runtime opencode`, `start --runtime opencode`, and `restart` of an OpenCode session fail before starting or replacing a container if either host OpenCode directory is missing, is not a directory, or is not readable and writable by the invoking host user.
- `agentbox` validates the OpenCode state directories only. It does not require a specific authentication file such as `auth.json`, because OpenCode may also be configured through environment variables or provider configuration.
- `agentbox` does not create, migrate, or write files inside the host OpenCode configuration or data directories.
- Codex sessions do not receive the OpenCode passthrough mounts.
