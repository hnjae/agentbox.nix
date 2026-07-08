# Runtime Workspace

## Workspace Mount

The canonical git root is bind-mounted at the same absolute host path inside the container.

Example:

- host git root: `/aaa/bbb`
- container git root mount: `/aaa/bbb`

This same absolute path rule is required so file paths emitted by the runtime match the host filesystem layout.

The runtime process runs as the image-local `user` account with UID `1000` and home `/home/user`. The runtime user's primary GID inside the container is mapped from the invoking host user's primary GID in Podman's user namespace. The runtime also preserves the invoking host user's supplemental group access using Podman's `keep-groups` behavior. A workspace file owned by, or group-writable for, the invoking host user must therefore be accessible to the runtime user according to normal host ownership and permission bits. `agentbox` must not mutate workspace ownership or permissions to achieve this.

## Launch Directory CWD

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

## Runtime Cache Volume

Each workspace identity has a writable runtime home at `/home/user`, backed by the Podman-managed named runtime cache volume.

Rules:

- The runtime user home inside the container is `/home/user`.
- `/home/user` is mounted as the runtime cache volume and persists across later one-shot runs or detached sessions for the same canonical git root.
- Standard XDG parent directories under `/home/user`, including `.config`, `.cache`, `.local`, and `.local/state`, are writable by the runtime user.
- Runtime state written under `/home/user` survives container recreation unless a documented runtime passthrough mount or workspace bind mount shadows that subpath.
- The runtime cache volume name is identical to the deterministic managed container name for the same workspace identity.
- The mounted runtime cache volume stores Nix cache, evaluation artifacts, the active runtime profile, and other runtime home state required to survive later one-shot runs or detached sessions for the same canonical git root.
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
- Once no container uses the cache volume, it remains available for explicit reclamation, for example with `agentbox clean --volumes` or direct Podman volume removal.
- Podman `--rm` removes the managed container, not the named runtime cache volume.
