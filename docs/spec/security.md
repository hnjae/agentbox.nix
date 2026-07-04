# Security

## Security And Isolation

Isolation expectations:

- separate rootless Podman container per transient run, foreground exec, or workspace session
- explicit workspace mount only for the canonical git root
- host-provided Nix inputs mounted alongside one Podman-managed cache volume
- Codex server sessions receive the selected host Codex home as a read-write passthrough mount, and foreground `exec` receives the default host `${HOME}/.codex` passthrough mount
- OpenCode sessions receive the invoking host user's OpenCode configuration and data directories as read-write passthrough mounts
- one writable Podman-managed named cache volume at `/home/user`
- minimal privileges
- networking enabled only as needed for the runtime command and, for detached sessions, the runtime server's local-only published attach endpoint
- attach endpoints bound to host loopback by default, not all host interfaces

Runtime user and bind-mount rules:

- The container runs as the non-root image-local `user` account with UID `1000` and home `/home/user`.
- The runtime user's primary GID is the invoking host user's primary GID as mapped by Podman's user namespace configuration.
- The invoking host user's supplemental groups are preserved for bind-mount permission checks using Podman's `keep-groups` behavior.
- The workspace bind mount is read-write by default.
- The Podman-managed runtime cache volume at `/home/user` is writable by the runtime user.
- Host ownership and permission bits remain authoritative.
- `agentbox` must not `chown`, `chmod`, remount, or elevate privileges to force access.
- If the runtime user cannot read or write a path that `agentbox` requires during container startup, `run`, `exec`, `start`, or `restart` fails clearly with the affected path and the permission problem.
- `agentbox` must not repair host workspace permissions by mutating the host mount.
- `agentbox` must not repair host Nix access by mutating host permissions or host configuration.

Unsupported security scope:

- hardened sandbox guarantees beyond normal rootless Podman isolation
- secret brokering or policy-based filesystem mediation
- cross-host or multi-user orchestration
