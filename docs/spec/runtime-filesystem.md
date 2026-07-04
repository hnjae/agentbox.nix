# Runtime Filesystem

## Runtime And Filesystem Behavior

### Workspace Mount

The canonical git root is bind-mounted at the same absolute host path inside the container.

Example:

- host git root: `/aaa/bbb`
- container git root mount: `/aaa/bbb`

This same absolute path rule is required so file paths emitted by the runtime match the host filesystem layout.

The runtime process runs as the image-local `user` account with UID `1000` and home `/home/user`. The runtime user's primary GID inside the container is mapped from the invoking host user's primary GID in Podman's user namespace. The runtime also preserves the invoking host user's supplemental group access using Podman's `keep-groups` behavior. A workspace file owned by, or group-writable for, the invoking host user must therefore be accessible to the runtime user according to normal host ownership and permission bits. `agentbox` must not mutate workspace ownership or permissions to achieve this.

### Launch Directory CWD

The effective working directory for a managed running session is the stored launch directory, not always the git root. `start` sets the launch directory from its canonical target directory. `restart` preserves the existing stored launch directory and starts the replacement server from that directory. `connect` uses the requested directory only to find the workspace session, then runs the host client from the stored launch directory. Transient `run` uses the canonical target directory as both the runtime server container working directory and the host-client process working directory, and does not create a stored launch directory.

Examples:

- command: `agentbox start --runtime opencode /aaa/bbb/subdir`
- mounted git root inside container: `/aaa/bbb`
- working directory seen by the runtime server: `/aaa/bbb/subdir`
- command: `agentbox connect /aaa/bbb/other`
- working directory of the host runtime client process: `/aaa/bbb/subdir`
- command: `agentbox restart /aaa/bbb/other`
- working directory seen by the replacement runtime server: `/aaa/bbb/subdir`
- command: `agentbox run --runtime opencode /aaa/bbb/subdir`
- working directory seen by the runtime server and host client: `/aaa/bbb/subdir`

Rules:

- `start` starts the runtime server from the canonical target directory inside the container and records that directory as the session launch directory.
- `restart` starts the replacement runtime server from the existing stored launch directory inside the container and records that same directory again as the session launch directory.
- `run` starts the transient runtime server from the canonical target directory inside the container, starts the runtime host client from the same canonical target directory on the host, and records no session metadata.
- `connect` starts the runtime host client from the stored launch directory on the host.
- `connect` does not change the already-running server process working directory.
- `restart` uses the requested target only to identify the managed session; it does not change the launch directory for the replacement session.
- To use a different launch directory for the same git root, the user stops the current session and starts a new one from the desired directory.
- Runtime-specific remote project behavior must be provided by the runtime client/server protocol, not by `podman attach` or `podman exec`.

### Runtime Cache Volume

Each workspace identity has a writable runtime home at `/home/user`, backed by the Podman-managed named runtime cache volume.

Rules:

- The runtime user home inside the container is `/home/user`.
- `/home/user` is mounted as the runtime cache volume and persists across later one-shot runs or detached sessions for the same canonical git root.
- Standard XDG parent directories under `/home/user`, including `.config`, `.cache`, `.local`, and `.local/state`, are writable by the runtime user.
- Runtime state written under `/home/user` survives container recreation unless a documented runtime passthrough mount or workspace bind mount shadows that subpath.
- The runtime cache volume name is identical to the deterministic managed container name for the same workspace identity.
- The mounted runtime cache volume stores Nix cache, evaluation artifacts, the active runtime profile, and other runtime home state that should survive later one-shot runs or detached sessions for the same canonical git root.
- A bind mount at `/home/user` does not satisfy the runtime cache volume requirement; the mount must be a Podman-managed named volume.
- The mounted runtime cache volume is owned or remapped so the runtime user can create home and cache files in it, including when a prior session created the volume under a different rootless Podman user namespace mapping.
- Existing named volumes are reused as-is when they satisfy the required `/home/user` named-volume mount contract.
- The runtime profile default path is `$XDG_STATE_HOME/nix/profile`.
- If `XDG_STATE_HOME` is unset and `HOME` is set, the runtime falls back to `$HOME/.local/state/nix/profile`.
- If both `XDG_STATE_HOME` and `HOME` are unavailable, the runtime falls back to `/home/user/.local/state/nix/profile`.
- The runtime profile environment is active for the container entrypoint and for login shells launched inside the runtime, including `sh`, `bash`, and `zsh`.
- Login shells restore the active runtime profile environment without materializing the profile or performing bootstrap validation.
- After runtime profile activation, `PATH` includes `$XDG_STATE_HOME/nix/profile/bin` and `/nix/var/nix/profiles/default/bin`, `NIX_PROFILES` includes `/nix/var/nix/profiles/default` and `$XDG_STATE_HOME/nix/profile`, and `XDG_DATA_DIRS` includes the matching `share` directories while preserving `/usr/local/share:/usr/share`.
- Runtime profile activation is idempotent. Re-entering `/entrypoint` or repeatedly sourcing login-shell startup files does not duplicate runtime profile entries in `PATH`, `NIX_PROFILES`, or `XDG_DATA_DIRS`.
- No other subpath under `/home/user` is required to persist unless it is named by a runtime-specific passthrough rule.
- `agentbox stop <directory>` does not explicitly delete the runtime cache volume.
- `agentbox restart <target>` preserves and reuses the named runtime cache volume for the selected managed session.
- Once no container uses the cache volume, it remains available for explicit reclamation, for example with `podman volume rm <container-name>` or `podman volume prune --all`.
- Podman `--rm` removes the managed container, not the named runtime cache volume.

### Host Git Identity Passthrough

For host passthrough rules, the launch repository is the resolved canonical git root for `run`, `start`, and `exec`, and the selected managed session's stored canonical git root for `restart`.

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

### Host Git Excludes File Passthrough

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

### SSH Commit Signing Passthrough

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

### Git SSH Remote Host Verification

When `run`, `start`, `restart`, or `exec` launches a container with a usable host SSH agent socket, `agentbox` also prepares SSH host-key verification for Git SSH remotes in the launch repository. This supports SSH remote authentication without mounting the host user's SSH directory, private keys, full Git configuration, or complete known_hosts files into the container.

The user may provide additional trusted host-key lines in strict JSON at `${XDG_CONFIG_HOME}/agentbox/config.json`, or at `${HOME}/.config/agentbox/config.json` when `XDG_CONFIG_HOME` is unset. The sample installed at `share/doc/agentbox/config.sample.json` shows the supported configuration fields.

``` json
{
  "knownHosts": [
    "github.com ssh-ed25519 AAAA...",
    "[git.example.com]:2222 ssh-ed25519 AAAA..."
  ],
  "defaultResourceLimits": {
    "cpus": 2,
    "memory": "8g"
  }
}
```

Rules:

- If the config file is missing, it is treated as an empty config.
- If the config path cannot be determined, `agentbox` prints a warning and continues with an empty config.
- Config JSON is strict. Parse errors, unknown top-level fields, a non-`knownHosts` or non-`defaultResourceLimits` schema, non-string `knownHosts` entries, blank entries, multiline entries, or invalid resource limits make the config incompatible.
- When config is incompatible, `agentbox` renames it to `config.json.bak.YYYYMMDDTHHMMSSZ` using UTC. If that backup path already exists, `agentbox` appends `.1`, `.2`, and so on until it finds an unused name. The incompatible config is then ignored for the launch.
- `agentbox` detects SSH Git remote hosts from the launch repository's configured remotes. SCP-like URLs such as `git@github.com:owner/repo.git`, `ssh://` URLs, and `git+ssh://` URLs are considered SSH remotes. HTTPS remotes and local paths are ignored.
- For each detected SSH remote host, `agentbox` runs `ssh -G <host>` to resolve the host's OpenSSH configuration. Host known_hosts lookup uses the resolved `hostname`, `port`, `hostkeyalias`, `userknownhostsfile`, and `globalknownhostsfile` values.
- If `ssh -G <host>` is unavailable or fails, host known_hosts lookup falls back to the detected SSH remote host and the invoking user's `$HOME/.ssh/known_hosts` and `$HOME/.ssh/known_hosts2` files.
- Host known_hosts lookup uses `ssh-keygen -F <host> -f <file>` so hashed known_hosts entries can match. Missing `ssh-keygen`, missing known_hosts files, no match, or unexpected lookup failures are non-fatal.
- If no known_hosts entry is found for a detected SSH remote host, `agentbox` prints a warning naming that host and continues launching.
- `agentbox` combines exact lines returned from matching host known_hosts lookups with config `knownHosts` entries, deduplicates exact duplicate lines, and writes the result to a temporary host file for the launch.
- If there is at least one known_hosts line, that temporary file is bind-mounted read-only at `/run/agentbox/known_hosts`, and the container receives `GIT_SSH_COMMAND=ssh -o UserKnownHostsFile=/run/agentbox/known_hosts -o StrictHostKeyChecking=yes`.
- The mounted `/run/agentbox/known_hosts` file contains the combined known_hosts lines for the lifetime of the container process that receives `GIT_SSH_COMMAND`.
- `agentbox` does not embed public keys for hosted Git providers, run `ssh-keyscan`, automatically trust remote hosts on first use, write known_hosts data to agentbox state, write known_hosts data under `/home/user`, modify the host user's known_hosts files, or mount the host user's `~/.ssh` directory by default.

### Codex Host Configuration Passthrough

Codex server sessions use the invoking host user's Codex configuration directory as the runtime Codex home. A non-empty `CODEX_HOME` overrides the default host Codex home for server sessions.

Rules:

- For `agentbox run --runtime codex`, `agentbox start --runtime codex`, and `agentbox restart` of a Codex session, if `CODEX_HOME` is set to a non-empty value, that host directory is bind-mounted read-write at the same absolute path inside the container and the server container receives `CODEX_HOME` with the same value.
- If `CODEX_HOME` is unset or empty for those Codex server launches, the host `${HOME}/.codex` directory is bind-mounted read-write at `/home/user/.codex`.
- The mount is required so auth refreshes, skills, MCP configuration, plugins, rules, and other Codex user state remain consistent between host and container Codex clients.
- `run --runtime codex`, `start --runtime codex`, and `restart` of a Codex session fail before starting or replacing a container if the selected host Codex home directory is missing, is not a directory, is not an absolute path, or is not readable and writable by the invoking host user.
- `agentbox exec` uses the default host `${HOME}/.codex` passthrough at `/home/user/.codex` and does not apply `CODEX_HOME` server passthrough.
- `agentbox` does not create, migrate, or write files inside the selected host Codex home directory.
- OpenCode sessions do not receive the Codex passthrough mount.

### OpenCode Host State Passthrough

OpenCode sessions use the invoking host user's OpenCode configuration and data directories as the runtime OpenCode state.

Rules:

- For `agentbox run --runtime opencode`, `agentbox start --runtime opencode`, and `agentbox restart` of an OpenCode session, the host `${XDG_CONFIG_HOME:-$HOME/.config}/opencode` directory is bind-mounted read-write at `/home/user/.config/opencode`.
- For `agentbox run --runtime opencode`, `agentbox start --runtime opencode`, and `agentbox restart` of an OpenCode session, the host `${XDG_DATA_HOME:-$HOME/.local/share}/opencode` directory is bind-mounted read-write at `/home/user/.local/share/opencode`.
- Both host directories are required so global configuration, provider settings, authentication state, and other OpenCode user state remain consistent between host and container OpenCode clients.
- `run --runtime opencode`, `start --runtime opencode`, and `restart` of an OpenCode session fail before starting or replacing a container if either host OpenCode directory is missing, is not a directory, or is not readable and writable by the invoking host user.
- `agentbox` validates the OpenCode state directories only. It does not require a specific authentication file such as `auth.json`, because OpenCode may also be configured through environment variables or provider configuration.
- `agentbox` does not create, migrate, or write files inside the host OpenCode configuration or data directories.
- Codex sessions do not receive the OpenCode passthrough mounts.

### Dev Environment Loading

When `run`, `start`, or `restart` uses the default `--dev-env auto`, it starts the runtime command through the first applicable development environment provider for the launch directory. For `run`, `start`, and `restart`, the runtime command is the runtime server inside the container. The provider priority is:

1. `direnv`
2. `devenv`
3. `nix develop`
4. no wrapper

Rules:

- `run` and `start` use the canonical target directory as the runtime process working directory even when a development environment wrapper is selected. `start` also records that directory as the session launch directory.
- `restart` uses the stored launch directory as the runtime process working directory and re-evaluates wrapper selection from that directory.
- `run --dev-env none`, `start --dev-env none`, and `restart --dev-env none` disable automatic development environment loading and start the runtime command directly.
- Only the selected provider is used. If the selected provider command is missing, blocked, exits unsuccessfully, or otherwise fails during runtime process startup, the container startup fails through the normal error path and `agentbox` does not silently try a lower-priority provider.
- Development environment provider commands are executed inside the runtime container. `agentbox` does not require host-side `direnv` or `devenv`.
- `connect` starts the runtime host client directly from the stored launch directory and does not re-evaluate `.envrc`, `devenv.nix`, or `flake.nix`.
- When `start` launches a session, the server environment is fixed by the launch directory and development environment selection used for that `start`.
- When `restart` replaces a session, the server environment is recomputed from the stored launch directory and selected `--dev-env` mode.
- `connect` to an already-running session does not reevaluate or replace the server environment.
- `agentbox` does not persist development environment selection or state for running-session compatibility checks.
- `agentbox` does not compare a different requested connect directory against the earlier `start` development environment context for that running session.

`direnv` selection:

- A matching `.envrc` applies when `.envrc` exists in the launch directory or in any ancestor up to and including the canonical git root.
- If a matching `.envrc` applies, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command as `direnv exec . <runtime argv>` from the launch directory.

`devenv` selection:

- `devenv` is considered only when no matching `.envrc` applies.
- The selected `devenv.nix` is the closest `devenv.nix` found in the launch directory or in any ancestor up to and including the canonical git root.
- If a `devenv.nix` is selected, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command as `devenv shell --no-tui -- <runtime argv>` from the launch directory, allowing `devenv` to resolve the nearest project configuration using its normal working-directory behavior.

`nix develop` selection:

- `nix develop` is considered only when no matching `.envrc` or `devenv.nix` applies.
- The selected flake is the closest `flake.nix` found in the launch directory or in any ancestor up to and including the canonical git root.
- Automatic flake selection considers only `devShells.<system>.<attr>`. `packages` and `legacyPackages` are not automatic development environment candidates.
- If the selected `flake.nix` is in the launch directory, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` looks for the `default` dev shell.
- If the selected `flake.nix` is in a parent directory of the launch directory, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` first looks for a dev shell named `basename(<directory>)`, then falls back to `default`.
- If a candidate dev shell exists, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command as `nix develop --no-write-lock-file path:<flake_root>#<attr> --command <runtime argv>`.
- If the selected flake can be evaluated but none of the candidate dev shells exists, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command directly.
- If automatic flake evaluation itself fails for reasons other than a missing candidate dev shell attribute, `run`, `start`, or `restart` fails clearly before starting or replacing a container.

### Runtime Server And Client Behavior

OpenCode managed sessions:

- use OpenCode's remote server and host-side connection client
- expose an `http` attach endpoint
- run with `OPENCODE_CONFIG_CONTENT={"autoupdate":false}` so OpenCode auto-update behavior does not change the installed runtime version inside the managed image or mutate host configuration
- run with `OPENCODE_PERMISSION='{"*":"allow"}'` so OpenCode receives an allow permission map for every permission key

Codex managed sessions:

- use Codex's app server and host-side remote client
- expose a `ws` attach endpoint
- use Codex WebSocket capability-token authentication on the app server and host-side remote client

Endpoint rules:

- The server listens inside the container on the runtime's configured listen address and container port.
- The runtime server command must pass the configured listen address to runtimes whose default bind address would not be reachable through the published attach endpoint.
- For OpenCode `http` attach endpoints, `run`, `start`, and `restart` treat the endpoint as ready only after `GET /global/health` on the same host-published endpoint returns `HTTP 200` and a JSON response body whose `healthy` field is `true`. A TCP connection, TCP accept followed by a reset, arbitrary HTTP response, malformed JSON response, or health response with `healthy: false` is not sufficient readiness.
- For Codex `ws` attach endpoints, `run`, `start`, and `restart` treat the endpoint as ready only after `GET /readyz` on the same host-published endpoint returns `HTTP 200`. A TCP connection alone is not sufficient readiness.
- The attach endpoint is published only on the host loopback interface by default.
- The default host attach IP is `127.0.0.1`.
- `agentbox` may let Podman allocate the host port, but it must discover the concrete host port from Podman before reporting success from `start` or `restart`, or executing the `run`, `start --connect`, `restart --connect`, or `connect` host client.
- The attach endpoint must be discoverable from the runtime's attach specification plus Podman's published port data. For managed sessions, stored managed-container metadata must also be consistent with that endpoint.
- The host client command is executed with inherited stdio. `run` executes it from the canonical target directory, while `start --connect`, `restart --connect`, and `connect` execute it from the running session's stored launch directory.
- When launching a host client for a loopback attach endpoint, `agentbox` preserves inherited proxy variables and ensures loopback hosts bypass proxies by augmenting both `NO_PROXY` and `no_proxy`.
- Codex app-server commands that listen on the container-wide attach address must use capability-token authentication. `agentbox` passes only the token SHA-256 to the container server command.
- For transient Codex `run`, the attach token is held only for the lifetime of the `agentbox run` process.
- For managed Codex `start` and `restart`, the attach token is stored under `$XDG_STATE_HOME/agentbox/codex/ws-tokens/` and read by later `agentbox connect`. Missing token state makes `connect` fail clearly and require session restart or recreation.

### Host-Attached Nix Model

The OpenCode and Codex runtimes use host-attached Nix support inside the container alongside a Podman-managed named Nix cache volume.

Rules:

- `/nix` is mounted into the container so the host Nix store and nix-daemon socket are available.
- `NIX_REMOTE=daemon` and the daemon socket at `/nix/var/nix/daemon-socket/socket` are part of the runtime contract.
- A host-compatible `nix` client is available in `PATH` inside the runtime.
- `/etc/nix` is mounted so host configuration and registry inheritance are visible inside the container.
- `/etc/static/nix` is mounted only when needed because `/etc/nix` resolves there on the host model.
- If a file under `/etc/nix`, such as `/etc/nix/nix.custom.conf`, points into `/etc/static/nix`, `run`, `start`, or `restart` treats `/etc/static/nix` as needed even when that static file ultimately resolves into `/nix/store`.
- Runtime profile state lives under `$XDG_STATE_HOME/nix/profile`, with fallback to `$HOME/.local/state/nix/profile` or `/home/user/.local/state/nix/profile` when needed.
- The Codex default image installs Codex from npm package `@openai/codex` at the version resolved by `agentbox` for that image build.
- The OpenCode default image installs OpenCode from npm package `opencode-ai` at the version resolved by `agentbox` for that image build.
- The runtime image provides its own CA bundle. Host SSL trust-store mounts are unsupported.
- If a host-attached Nix prerequisite is missing, `run`, `start`, or `restart` fails clearly and does not attempt to synthesize a bundled Nix installation.
- If the selected runtime host client command is missing, `run` fails clearly before starting a transient container.
- If `restart --connect` is selected and the selected runtime host client command is missing, `restart` fails before stopping the old container.
