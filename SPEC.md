# Agentbox Specification

This document describes the behavior that users and operators can observe from
the `agentbox` CLI, installed package, managed Podman objects, and documented
filesystem effects. It does not specify internal module structure.

## Summary

`agentbox` is a Rust CLI for running code agents inside isolated Podman
containers.

The MVP is workspace-centric rather than name-centric:

- `agentbox run --runtime <opencode|codex> <directory>`
- `agentbox attach <directory>`
- `agentbox ls`
- `agentbox stop <directory>`

`agentbox run --runtime <opencode|codex> <directory>` resolves `<directory>` to
its canonical git root and launches one managed workspace session for that
repository as a detached runtime server container. The container main process is
the selected runtime's remote server command.

`agentbox attach <directory>` discovers the running server endpoint for the
resolved repository and runs the selected runtime's host-side client command
from the requested target directory.

Managed containers are started with Podman's `--rm` and `--rmi` cleanup flags.
When the runtime server exits, or when `agentbox stop <directory>` stops it,
Podman removes the stopped container and removes the image when possible. The
named runtime cache volume is intentionally left for explicit later cleanup.

If a matching managed session already exists for a repository, `run` fails
clearly instead of reusing, replacing, or changing it.

MVP runtime support includes OpenCode and Codex.

## Supported Environment And Limits

Supported host environments:

- NixOS
- other Linux distributions with multi-user Nix

Required host tools:

- Podman
- Git
- a host `nix` client and nix-daemon socket compatible with the host-attached
  Nix model described below

The target directory is expected to live inside a git repository. A non-git
target fails clearly; the MVP does not create ad-hoc non-git sessions.

Out of scope for the MVP:

- macOS or Windows support
- more than one valid managed session for the same canonical git root
- silent runtime switching or automatic recreation when an existing managed
  session has invalid runtime metadata
- user-supplied runtime image references
- generic container orchestration
- durable runtime state beyond the workspace bind mount, one Podman-managed
  runtime cache volume, and live managed-container metadata
- a bundled standalone Nix installation inside the runtime image
- stopped managed containers that remain after the runtime server exits

## Workspace Identity And Path Resolution

A workspace session is the single valid managed agent environment for one
canonical git root.

`agentbox` resolves `<directory>` using these rules:

1. Convert the user input to an absolute path.
2. Resolve the git root with `git -C <directory> rev-parse --show-toplevel`.
3. Canonicalize the resulting git root by resolving symlinks.
4. Canonicalize the requested target directory as well.
5. Require the target directory to remain inside the canonical git root.

Required behavior:

- A symlinked path resolves to the same canonical git root as the real path.
- Nested repositories use the git root reported for the requested target
  directory, so an inner repository gets its own session.
- Submodules and git worktrees each get their own session identity because each
  resolves to its own canonical git root.
- Moving a repository to a different absolute path creates a new identity.
- A still-running container whose stored git-root path no longer exists is
  reported as `orphaned` until it is stopped.
- The requested target directory is not part of identity. Different `run` or
  `attach` invocations may target different subdirectories under the same git
  root while still referring to the same running managed session.

## Naming And Visible Podman State

Each workspace session has a deterministic logical name derived from the
canonical git root.

Naming algorithm:

1. Take the canonical git-root absolute path bytes.
2. Compute `SHA-256` of those bytes.
3. Use the first 12 lowercase hex characters of the digest as `hash12`.
4. Escape the canonical git-root path by replacing `/` with `_`.
5. Replace every other character outside `[A-Za-z0-9_.-]` with `-`.
6. Build the readable suffix from the escaped path by taking the full escaped
   path if it already fits, or else taking the rightmost characters of the
   escaped path so the final name fits within 63 characters including prefix,
   separator, and `hash12`.
7. Container names use the prefix `agentbox-`.
8. If the derived name is already occupied by a non-matching Podman object,
   fail with a name-conflict error. Do not generate an alternate name.

Required behavior:

- The escaped readable suffix remains visible in the final name.
- Overlong paths preserve the rightmost path segment characters, not the
  leftmost prefix characters.
- The exact same canonical git-root path always yields the exact same container
  name.
- Runtime is not part of identity because only one session per repository is
  allowed.
- The runtime cache volume name for a workspace session is exactly the same
  string as the container name.
- The algorithm does not depend on ambient Podman state to produce a different
  name.
- The 63-character maximum is owned by this spec for managed container names.
- If two different canonical git roots collide on `hash12`, fail clearly with a
  hash-collision error rather than treating them as the same workspace.

Example:

- canonical git root: `/aaa/bbb`
- escaped readable suffix: `_aaa_bbb`
- container name: `agentbox-_aaa_bbb-9ae5447864f7`
- runtime cache volume name: `agentbox-_aaa_bbb-9ae5447864f7`

Managed containers are visible through Podman while they are running. They carry
Podman labels that identify at least:

- that the container is managed by `agentbox`
- the metadata schema version
- the canonical git root
- the git-root `hash12`
- the selected runtime
- the default runtime image reference used for the session
- the logical name
- the attach endpoint scheme
- the runtime server container port and listen address

`agentbox` discovers sessions from live Podman state. It does not require a
separate host-side session database.

## CLI

### `agentbox run --runtime <opencode|codex> <directory>`

`run` launches a new workspace session as a detached runtime server.

Required flag:

- `--runtime <opencode|codex>`

Expected behavior:

1. Validate Git availability and resolve `<directory>` to a canonical git root
   and canonical target directory.
2. Validate Podman, the selected runtime, and the host-attached Nix
   prerequisites.
3. Ensure concurrent lifecycle operations for the same git root do not create
   duplicate containers or race teardown.
4. Discover existing managed containers for that canonical git root.
5. If more than one matching container exists, fail as `duplicate` and do not
   guess which one to use.
6. If exactly one matching managed container exists, fail clearly instead of
   reusing or replacing it. For a healthy running session, suggest
   `agentbox attach <directory>` or `agentbox stop <directory>`.
7. If none exists, start detached `podman run --rm --rmi` with the required
   labels, mounts, default runtime image, local-only published attach endpoint,
   and target-directory working directory.
8. Run the selected runtime's actual remote server command as the container main
   command.
9. Wait until the runtime server endpoint is reachable or the container exits.
10. Report the discovered attach endpoint and suggest
    `agentbox attach <directory>`.

Runtime rules:

- `run` accepts only `opencode` and `codex` in the MVP.
- `--runtime` selects the runtime for the new session.
- If a managed session already exists for the resolved git root, `run` fails
  before reusing or comparing any stored runtime value.
- `--runtime` does not change session identity.

Image rules:

- `run` does not accept a user-supplied image reference.
- `run` always uses the selected runtime's default image reference.
- The default image may be built or reused by `agentbox`; users do not need to
  supply a build context.
- `agentbox` records the exact default image reference on the running managed
  container so live discovery can report it while the container exists.
- Podman owns image removal behavior for `--rmi`; `agentbox` does not perform
  extra image pruning or monitor later image removal outcomes.

### `agentbox attach <directory>`

`attach` connects to an already-running managed workspace session.

Expected behavior:

1. Resolve `<directory>` to a canonical git root and canonical target directory.
2. Discover the managed container for that canonical git root.
3. Fail if no matching managed session exists, and suggest
   `agentbox run --runtime <opencode|codex> <directory>`.
4. Fail as `duplicate` if more than one matching container exists.
5. Fail if the matching container is not running.
6. Discover the runtime attach endpoint from managed-container metadata and
   Podman's published port data.
7. Execute the runtime host client command from the canonical target directory
   with stdio inherited.

Rules:

- `attach` never creates a new session.
- `attach` never starts or restarts a stopped session.
- `attach` never prompts for runtime selection.
- `attach` does not accept or interpret `--runtime`.
- `attach` does not accept or interpret `--image`.
- `attach` does not use `podman attach` as the user transport in the MVP.
- `attach` does not open a raw shell through `podman exec`.
- The host client process current working directory is the requested target
  directory.
- The running server process keeps the working directory and environment from
  its original `run`.
- If the runtime client cannot be found on the host, `attach` fails clearly with
  the required command name.

### `agentbox ls`

`ls` lists managed workspace sessions from live Podman discovery.

Expected output fields:

- canonical git root
- runtime
- status
- concrete container name

Status values:

- `running`: the managed container exists and is running.
- `orphaned`: the managed container exists and is running, but the stored git
  root path no longer exists on the host.
- `duplicate`: more than one managed container claims the same canonical git
  root.
- `failed`: the managed container exists, but required metadata, workspace
  mounts, published endpoint data, or other inspectable session invariants are
  inconsistent.

Rules:

- Containers not marked as managed by `agentbox` are ignored, even if their
  names resemble `agentbox` names.
- `ls` prints a compact human-readable table in the MVP.
- The MVP does not require machine-readable `ls` output.

### `agentbox stop <directory>`

`stop` stops the workspace session for the resolved repository. It is an
idempotent stop command for live managed containers, including orphaned live
containers. It is not a volume pruning command.

Expected behavior:

1. Resolve `<directory>` to a canonical git root.
2. If `<directory>` does not exist, allow an exact absolute git-root path string
   to match an orphaned session directly.
3. Ensure concurrent lifecycle operations for the same git root do not race.
4. Stop the matching container if it is running.
5. Treat an already-removed matching container as success after verifying it is
   absent.
6. Rely on Podman's `--rm --rmi` cleanup for container and image removal after
   the stop.
7. Leave the runtime cache volume unmanaged by `stop` so it can be reclaimed
   later by explicit Podman volume cleanup.

Optional flag:

- `--force`: best-effort cleanup when duplicate exact matches exist

Safety rules:

- `stop` never deletes the user workspace.
- `stop` never directly removes images or named cache volumes.

## Completion And Installed Assets

Shell completion for `attach` and `stop` is dynamic.

Required behavior:

- Completion candidates come from live managed sessions, not from a static file.
- Candidate values are canonical git root paths.
- Candidate descriptions include runtime and status when the shell supports
  descriptions.
- Running sessions are visible immediately at tab completion time.
- `fzf-tab`-style frontends work automatically because they consume normal shell
  completion results.

The default Nix package installs shell completion and manual assets alongside
the `agentbox` binary.

Required package output paths:

- `share/bash-completion/completions/agentbox`
- `share/zsh/site-functions/_agentbox`
- `share/fish/vendor_completions.d/agentbox.fish`
- `share/man/man1/agentbox.1`, or `share/man/man1/agentbox.1.gz` when the Nix
  fixup phase compresses manual pages

`nix build '.#default'` must produce those files in its result path.

## Runtime And Filesystem Behavior

### Workspace Mount

The canonical git root is bind-mounted at the same absolute host path inside the
container.

Example:

- host git root: `/aaa/bbb`
- container git root mount: `/aaa/bbb`

This same absolute path rule is required so file paths emitted by the runtime
match the host filesystem layout.

### Target Directory CWD

The effective working directory for a given `run` or `attach` invocation is the
requested target directory, not always the git root.

Examples:

- command: `agentbox run --runtime opencode /aaa/bbb/subdir`
- mounted git root inside container: `/aaa/bbb`
- working directory seen by the runtime server: `/aaa/bbb/subdir`
- command: `agentbox attach /aaa/bbb/other`
- working directory of the host runtime client process: `/aaa/bbb/other`

Rules:

- `run` starts the runtime server from the requested target directory inside the
  container.
- `attach` starts the runtime host client from the requested target directory on
  the host.
- `attach` does not change the already-running server process working
  directory.
- Runtime-specific remote project behavior must be provided by the runtime
  client/server protocol, not by `podman attach` or `podman exec`.

### Runtime Cache Volume

Each workspace session has a writable runtime home at `/home/user`, but only
`/home/user/.cache/nix` is backed by a Podman-managed named volume.

Rules:

- The runtime user home inside the container is `/home/user`.
- `/home/user` itself is writable for runtime state creation, but it is not
  required to persist across container recreation.
- The runtime cache volume name is identical to the container name for the same
  workspace session.
- The mounted runtime cache volume stores Nix cache and evaluation artifacts
  that should survive later sessions for the same canonical git root.
- The active runtime profile does not live under the cache volume.
- The runtime profile default path is `$XDG_STATE_HOME/nix/profile`.
- If `XDG_STATE_HOME` is unset and `HOME` is set, the runtime falls back to
  `$HOME/.local/state/nix/profile`.
- If both `XDG_STATE_HOME` and `HOME` are unavailable, the runtime falls back to
  `/home/user/.local/state/nix/profile`.
- No other subpath under `/home/user` is required to persist in the MVP.
- `agentbox stop <directory>` does not explicitly delete the runtime cache
  volume.
- Once no container uses the cache volume, it remains available for explicit
  reclamation, for example with `podman volume rm <container-name>` or
  `podman volume prune --all`.
- Podman `--rm` removes the managed container, not the named runtime cache
  volume.

### Direnv

When the target directory uses `direnv`, the runtime command for that invocation
is executed from the target directory context as:

- `direnv exec . <runtime-server>` for `run`
- `direnv exec . <runtime-client>` for `attach`, when the host target directory
  uses `direnv`

Rules:

- `direnv` evaluation happens relative to the requested target directory, not
  forcibly at the git root.
- `direnv` wraps the runtime server command for `run`.
- `direnv` wraps the runtime host client command for `attach` when a host-side
  `.envrc` applies to the requested target directory.
- When `run` launches a session, the server environment is fixed by the
  requested target directory used for that `run`.
- `attach` to an already-running session does not reevaluate or replace the
  server environment.
- The MVP does not persist host-side direnv state for running-session
  compatibility checks.
- The MVP does not compare the requested attach target directory against the
  earlier `run` direnv context for that running session.
- If `.envrc` is present but `direnv` is unavailable, blocked, or fails to load,
  the affected `run` or `attach` fails clearly.
- If no `.envrc` applies, the runtime server or client launches normally.

### Runtime Server And Client Commands

OpenCode command contract:

- server command inside the container: `opencode serve --port <container-port>`
- host client command: `opencode attach "http://<host-ip>:<host-port>"`
- attach endpoint scheme: `http`

Codex command contract:

- server command inside the container:
  `codex --dangerously-bypass-approvals-and-sandbox app-server --listen 'ws://<container-listen-ip>:<container-port>'`
- host client command: `codex --remote 'ws://<host-ip>:<host-port>'`
- attach endpoint scheme: `ws`

Endpoint rules:

- The server listens inside the container on the runtime's configured listen
  address and container port.
- The attach endpoint is published only on the host loopback interface by
  default.
- The default host attach IP is `127.0.0.1`.
- `agentbox` may let Podman allocate the host port, but it must discover the
  concrete host port from Podman before reporting success from `run` or
  executing `attach`.
- The attach endpoint must be discoverable from live managed-container metadata
  plus Podman's published port data.
- The host client command is executed with inherited stdio from the requested
  target directory.

### Host-Attached Nix Model

The OpenCode and Codex runtimes in the MVP use host-attached Nix support inside
the container alongside a Podman-managed named Nix cache volume.

Rules:

- `/nix` is mounted into the container so the host Nix store and nix-daemon
  socket are available.
- `NIX_REMOTE=daemon` and the daemon socket at
  `/nix/var/nix/daemon-socket/socket` are part of the runtime contract.
- A host `nix` client is available in `PATH`, commonly mounted at
  `/usr/local/bin/nix`.
- `/etc/nix` is mounted so host configuration and registry inheritance are
  visible inside the container.
- `/etc/static/nix` is mounted only when needed because `/etc/nix` resolves
  there on the host model.
- Runtime profile state lives under `$XDG_STATE_HOME/nix/profile`, with fallback
  to `$HOME/.local/state/nix/profile` or `/home/user/.local/state/nix/profile`
  when needed.
- The runtime image provides its own CA bundle. Host SSL trust-store mounts are
  out of scope for the MVP.
- If a host-attached Nix prerequisite is missing, `run` fails clearly and does
  not attempt to synthesize a bundled Nix installation.

## Lifecycle And Drift Recovery

Valid lifecycle behavior:

- `run` creates the workspace session as a detached runtime server container.
- `attach` discovers an existing running workspace session and runs the runtime
  host client against its published endpoint.
- `ls` derives session status from live Podman state and host path checks.
- `stop` stops the container and relies on the container's `--rm --rmi` run
  options for cleanup.
- Concurrent lifecycle operations for the same canonical git root are
  serialized so they do not create duplicate containers or race teardown.
- Stale coordination state with no live owner is cleared automatically before
  proceeding.

Image lifecycle is tied to the managed session. When the runtime server exits or
`agentbox stop` stops it, Podman removes the image if no other container
prevents removal. `agentbox` does not run separate image-pruning commands.

Named runtime cache volume lifecycle remains separate. `agentbox stop` leaves
the workspace cache volume intact so later sessions can reuse it. Volume
reclamation is explicit, for example `podman volume rm <container-name>` or
`podman volume prune --all`.

Required drift behavior:

- Duplicate containers for one git root: mark the session as `duplicate`, fail
  `run` and `attach`, and do not guess which container to use.
- Missing or malformed managed-container metadata: mark the session as `failed`
  and require explicit repair or recreation before the session can be used
  again.
- Missing runtime cache volume mount for an existing session: fail clearly and
  require explicit container recreation.
- Missing or inconsistent attach endpoint metadata or published port data: mark
  the session as `failed` and require explicit repair or recreation before the
  session can be attached.
- Missing host-attached Nix prerequisite: fail clearly, report the missing
  mount, client, socket, config, or state-path requirement, and do not attempt
  to synthesize a bundled Nix installation.
- Runtime image setup failure: fail clearly and preserve inspectable runtime
  state when Podman has not already removed the container.
- Hash collision between different canonical git roots: fail clearly and do not
  treat them as the same workspace.
- Stop failure: report exactly which managed containers are still running or
  still inspectable.

## Error Handling

The CLI must produce actionable errors that say what failed, which workspace was
involved, which external command failed when relevant, and what the user can try
next.

Required error cases:

- non-git target directory
- requested directory escapes the resolved git root
- unsupported runtime
- Podman not installed
- Git not installed
- unsupported or malformed runtime metadata on an existing managed session
- container failed to start
- runtime server command not found
- runtime host client command not found
- attach failed
- missing or inconsistent attach endpoint metadata
- missing published attach port
- duplicate managed containers for one git root
- `run` called for a git root that already has a managed session
- name conflict with a non-matching Podman object
- hash collision between different canonical git roots
- missing required managed-container metadata on an existing session
- coordination state that cannot be cleared automatically
- missing runtime cache volume mount for an existing session
- orphaned session after repo move
- missing host `nix` client in `PATH`
- missing nix-daemon socket at `/nix/var/nix/daemon-socket/socket`
- missing `/etc/nix` host mount or unreadable `/etc/nix/nix.conf`
- missing readable `/etc/static/nix` target when `/etc/nix` resolves there
- unusable runtime profile path under the XDG state or HOME fallback location
- runtime image setup failure
- `direnv` unavailable, blocked, or failing when a matching `.envrc` applies
- workspace or host Nix permission problems that prevent required access

## Security And Isolation

MVP isolation expectations:

- separate rootless Podman container per workspace session
- explicit workspace mount only for the canonical git root
- host-provided Nix inputs mounted alongside one Podman-managed cache volume
- one writable Podman-managed named cache volume at `/home/user/.cache/nix`
- minimal privileges
- networking enabled only as needed for the runtime server and its local-only
  published attach endpoint
- attach endpoints bound to host loopback by default, not all host interfaces

Runtime user and bind-mount rules:

- The container runs as the non-root `user` account with home `/home/user`.
- The workspace bind mount is read-write by default.
- The Podman-managed runtime cache volume at `/home/user/.cache/nix` is writable
  by the runtime user.
- Host ownership and permission bits remain authoritative.
- `agentbox` must not `chown`, `chmod`, remount, or elevate privileges to force
  access.
- If the runtime user cannot read or write a required path inside the bind
  mount, `run` or `attach` fails clearly with the affected path and the
  permission problem.
- `agentbox` must not repair host workspace permissions by mutating the host
  mount.
- `agentbox` must not repair host Nix access by mutating host permissions or
  host configuration.

Out of scope for MVP:

- hardened sandbox guarantees beyond normal rootless Podman isolation
- secret brokering or policy-based filesystem mediation
- cross-host or multi-user orchestration
