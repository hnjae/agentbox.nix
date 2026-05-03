# Agentbox Specification

This document specifies user-visible CLI behavior and operator-visible runtime
state for the `agentbox` CLI, installed package, managed Podman objects, and
documented filesystem effects. It does not specify Rust module structure or
private implementation details.

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
repository as a detached runtime server container. The container starts the
selected runtime server for that workspace.

`agentbox attach <directory>` discovers the running server endpoint for the
resolved repository and runs the running session's runtime host-side client
command from the session's stored launch directory. The newly requested
directory is used only to identify the workspace once the session exists. For
`attach`, `<directory>` is a workspace selector, not a request to change the
running session's working directory.

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

Always-required host tools:

- Podman
- Git
- a host `nix` client and nix-daemon socket compatible with the host-attached
  Nix model described below

Conditionally required host tools:

- the running session's runtime host client command for `attach`
- `direnv` when a matching `.envrc` applies to the directory whose environment
  is used by the command: the `run` target directory for server startup, or the
  stored launch directory for `attach`

For `run` and `attach`, `<directory>` must resolve to an existing directory
inside a git repository. A non-git target fails clearly; the MVP does not create
ad-hoc non-git sessions. `stop` normally follows the same resolution rules, but
it may also accept an exact absolute git-root path string for an orphaned
session whose stored path no longer exists.

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
4. Canonicalize the target directory as well.
5. Require the target directory to remain inside the canonical git root.

After resolution, "target directory" means this canonical target directory, not
the raw path spelling entered by the user.

When `run` successfully launches a session, the canonical target directory
becomes the session's launch directory. The launch directory is recorded with the
session and remains the stable working-directory and `direnv` context for later
attaches to that running session.

Required behavior:

- A symlinked path resolves to the same canonical git root as the real path.
- Nested repositories use the git root reported for the target directory, so an
  inner repository gets its own session.
- Submodules and git worktrees each get their own session identity because each
  resolves to its own canonical git root.
- Moving a repository to a different absolute path creates a new identity.
- A still-running container whose stored git-root path no longer exists is
  reported as `orphaned` until it is stopped.
- The target directory is not part of identity. A `run` invocation may choose any
  subdirectory under the git root as the launch directory for a new session, and
  an `attach` invocation may provide any subdirectory under the same git root to
  find that session.
- `attach` target directories do not retarget a running session. They identify
  the workspace session, then the running session's stored launch directory
  controls host-client working directory and environment.

## Naming And Visible Podman State

Each workspace session has a deterministic logical name derived from the
canonical git root.

Name contract:

- Container names use the prefix `agentbox-`.
- The final name includes a readable suffix derived from the canonical git root.
- Overlong paths preserve the rightmost path segment characters, not the
  leftmost prefix characters.
- The exact same canonical git-root path always yields the exact same container
  name.
- Runtime is not part of identity because only one session per repository is
  allowed.
- The runtime cache volume name for a workspace session is exactly the same
  string as the container name.
- Ambient Podman state does not cause the same canonical git root to produce a
  different name.
- The 63-character maximum is owned by this spec for managed container names.
- If the derived name is already occupied by a non-matching Podman object, fail
  with a name-conflict error. Do not generate an alternate name.
- If two different canonical git roots would produce indistinguishable managed
  identities, fail clearly with an identity-collision error rather than treating
  them as the same workspace.

Example shape:

- canonical git root: `/aaa/bbb`
- readable suffix: `_aaa_bbb`
- container name starts with `agentbox-_aaa_bbb-`
- runtime cache volume name is the same string as the container name

Managed containers are visible through Podman while they are running. They carry
Podman labels that identify at least:

- that the container is managed by `agentbox`
- the metadata schema version
- the canonical git root
- a stable git-root identity token
- the selected runtime
- the default runtime image reference used for the session
- the canonical session launch directory
- the logical name
- the attach endpoint scheme
- the runtime server container port and listen address

`agentbox` discovers sessions from live Podman state. It does not require a
separate host-side session database.

## CLI

Global flags:

- `--verbose` enables diagnostic command traces and external command output for
  commands that support verbose diagnostics. Diagnostic output is written to
  stderr and must not replace machine-readable or success output on stdout.

### `agentbox run --runtime <opencode|codex> <directory>`

`run` launches a new workspace session as a detached runtime server.

Required flag:

- `--runtime <opencode|codex>`

Expected behavior:

1. Validate Git availability and resolve `<directory>` to a canonical git root
   and canonical target directory.
2. Validate Podman, the selected runtime, and the host-attached Nix
   prerequisites.
3. Ensure concurrent lifecycle operations for the same git root do not leave
   duplicate sessions or ambiguous lifecycle state.
4. Discover existing managed containers for that canonical git root.
5. If more than one matching container exists, fail as `duplicate` and do not
   guess which one to use.
6. If exactly one matching managed container exists, fail clearly instead of
   reusing or replacing it. For a healthy running session, suggest
   `agentbox attach <directory>` or `agentbox stop <directory>`.
7. If none exists, record the canonical target directory as the session launch
   directory and start detached `podman run --rm --rmi` with the required labels,
   mounts, default runtime image, local-only published attach endpoint, and
   launch-directory working directory.
8. Start the selected runtime server for the session. If `direnv` applies, the
   server starts with the launch directory's `direnv` environment.
9. Wait until the runtime server endpoint is reachable or the container exits.
10. Report the discovered attach endpoint and suggest
    `agentbox attach <directory>`.

Progress and diagnostics:

- `run` prints short phase progress to stderr while checking prerequisites,
  resolving session state, ensuring the runtime image, starting the detached
  container, and waiting for the runtime server endpoint.
- `run` keeps its final success message on stdout.
- With `--verbose`, `run` also prints the external commands it executes and
  forwards non-JSON Podman command output to stderr.
- If the runtime container fails to start, exits before readiness, or times out
  before becoming reachable, `run` includes a short `podman logs --tail` excerpt
  for the managed container when Podman can provide one.

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

`attach` connects to an already-running managed workspace session. For
`attach`, `<directory>` is a workspace selector, not a requested working
directory for the running session.

Expected behavior:

1. Resolve `<directory>` to a canonical git root and canonical requested
   directory.
2. Discover the managed container for that canonical git root.
3. Fail if no matching managed session exists, and suggest
   `agentbox run --runtime <opencode|codex> <directory>`.
4. Fail as `duplicate` if more than one matching container exists.
5. Fail if the matching container is not running.
6. Discover the runtime attach endpoint and stored launch directory from
   managed-container metadata and Podman's published port data.
7. If the canonical requested directory differs from the stored launch
   directory, report that the requested directory was used only to identify the
   workspace and that `attach` is using the stored launch directory.
8. Execute the runtime host client command from the stored launch directory with
   stdio inherited.

Rules:

- `attach` never creates a new session.
- `attach` never starts or restarts a stopped session.
- `attach` never prompts for runtime selection.
- `attach` does not accept or interpret `--runtime`.
- `attach` does not accept or interpret `--image`.
- `attach` does not use `podman attach` as the user transport in the MVP.
- `attach` does not open a raw shell through `podman exec`.
- The host client process current working directory is the running session's
  stored launch directory.
- When the requested directory differs from the stored launch directory,
  `attach` prints a short notice before launching the host client.
- The running server process keeps the working directory and environment from
  its original `run`.
- A different requested directory under the same git root does not change the
  running server or host client working directory for that `attach`.
- If the runtime client cannot be found on the host, `attach` fails clearly with
  the required command name.

### `agentbox ls`

`ls` lists managed workspace sessions from live Podman discovery.

Expected output fields:

- canonical git root, or `unknown`
- runtime, or `unknown`
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
- For `failed` sessions, fields that cannot be recovered from live Podman state
  are shown as `unknown`. The concrete container name must still be shown so the
  user or operator can inspect or remove the broken container.
- `ls` prints a compact human-readable table in the MVP.
- The MVP does not require machine-readable `ls` output.

### `agentbox stop <directory>`

`stop` stops the workspace session for the resolved repository, including
orphaned live containers. It is not a volume pruning command.

Expected behavior:

1. If `<directory>` exists, resolve it to a canonical git root.
2. If `<directory>` does not exist, require an exact absolute git-root path
   string and match only a live orphaned session whose stored git-root path is
   exactly that string.
3. Ensure concurrent lifecycle operations for the same git root do not race.
4. Stop the matching container if it is running.
5. Treat an already-removed matching container as success after verifying it is
   absent.
6. If no matching managed session exists, report that no session exists for the
   resolved repository or exact orphan path and exit non-zero.
7. Rely on Podman's `--rm --rmi` cleanup for container and image removal after
   the stop.
8. Leave the runtime cache volume unmanaged by `stop` so it can be reclaimed
   later by explicit Podman volume cleanup.

Optional flag:

- `--force`: best-effort cleanup when duplicate or failed exact matches exist

Safety rules:

- Without `--force`, `stop` fails when more than one matching managed container
  is found.
- With `--force`, `stop` stops all live managed containers that exactly claim
  the resolved canonical git root or exact orphan path. It still does not stop
  containers that cannot be matched to that identity.
- `stop` never deletes the user workspace.
- `stop` never directly removes images or named cache volumes.

## Completion And Installed Assets

Shell completion for `attach` and `stop` is dynamic.

Required behavior:

- Completion candidates come from live managed sessions, not from a static file.
- Candidate values are canonical or stored git root paths when known. Sessions
  with no recoverable git-root path are not completion candidates, but remain
  visible through `agentbox ls` by concrete container name.
- `attach` completion includes only attachable `running` sessions with valid
  endpoint metadata.
- `stop` completion includes running, orphaned, duplicate, and failed sessions
  when a canonical or stored git-root path is known.
- Candidate descriptions include runtime and status when the shell supports
  descriptions.
- Eligible live sessions are reflected immediately at tab completion time.
- `fzf-tab`-style frontends work automatically because they consume normal shell
  completion results.

The default Nix package installs shell completion and manual assets alongside
the `agentbox` binary.

Required package output paths:

- `share/bash-completion/completions/agentbox`
- `share/zsh/site-functions/_agentbox`
- `share/fish/vendor_completions.d/agentbox.fish`
- `share/man/man1/agentbox.1`, `share/man/man1/agentbox-run.1`,
  `share/man/man1/agentbox-attach.1`, `share/man/man1/agentbox-ls.1`,
  `share/man/man1/agentbox-stop.1`, and
  `share/man/man1/agentbox-completion.1`, or matching `.gz` files when the Nix
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

### Launch Directory CWD

The effective working directory for a running session is the stored launch
directory, not always the git root. `run` sets the launch directory from its
canonical target directory. `attach` uses the requested directory only to find
the workspace session, then runs the host client from the stored launch
directory.

Examples:

- command: `agentbox run --runtime opencode /aaa/bbb/subdir`
- mounted git root inside container: `/aaa/bbb`
- working directory seen by the runtime server: `/aaa/bbb/subdir`
- command: `agentbox attach /aaa/bbb/other`
- working directory of the host runtime client process: `/aaa/bbb/subdir`

Rules:

- `run` starts the runtime server from the canonical target directory inside the
  container and records that directory as the session launch directory.
- `attach` starts the runtime host client from the stored launch directory on
  the host.
- `attach` does not change the already-running server process working
  directory.
- To use a different launch directory for the same git root, the user stops the
  current session and runs a new one from the desired directory.
- Runtime-specific remote project behavior must be provided by the runtime
  client/server protocol, not by `podman attach` or `podman exec`.

### Runtime Cache Volume

Each workspace session has a writable runtime home at `/home/user`, but only
`/home/user/.cache/nix` is backed by a Podman-managed named volume.

Rules:

- The runtime user home inside the container is `/home/user`.
- `/home/user` itself is writable for runtime state creation, but it is not
  required to persist across container recreation.
- Runtime state outside `/home/user/.cache/nix` is ephemeral in the MVP. Users
  should not expect runtime configuration, login state, shell history, or files
  written elsewhere under `/home/user` to survive container recreation unless
  those files are also stored in the workspace bind mount.
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

When a command uses a directory with `direnv`, the affected runtime process is
launched with the environment produced for the directory that command actually
uses.

Rules:

- `direnv` evaluation happens relative to the effective command directory, not
  forcibly at the git root.
- `run` starts the runtime server in the target directory's `direnv`
  environment when a matching `.envrc` applies.
- `attach` starts the runtime host client in the stored launch directory's
  host-side `direnv` environment when a matching `.envrc` applies.
- When `run` launches a session, the server environment is fixed by the
  launch directory used for that `run`.
- `attach` to an already-running session does not reevaluate or replace the
  server environment.
- The MVP does not persist host-side direnv state for running-session
  compatibility checks.
- The MVP does not compare a different requested attach directory against the
  earlier `run` direnv context for that running session.
- If `.envrc` is present but `direnv` is unavailable, blocked, or fails to load,
  the affected `run` or `attach` fails clearly.
- If no `.envrc` applies, the runtime server or client launches normally.

### Runtime Server And Client Behavior

OpenCode sessions:

- use OpenCode's remote server and host-side attach client
- expose an `http` attach endpoint

Codex sessions:

- use Codex's app server and host-side remote client
- expose a `ws` attach endpoint

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
- The host client command is executed with inherited stdio from the running
  session's stored launch directory.

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
- If a file under `/etc/nix`, such as `/etc/nix/nix.custom.conf`, points into
  `/etc/static/nix`, `run` treats `/etc/static/nix` as needed even when that
  static file ultimately resolves into `/nix/store`.
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
- Concurrent lifecycle operations for the same canonical git root do not leave
  more than one valid managed session or ambiguous cleanup outcome.

Image lifecycle is tied to the managed session. When the runtime server exits or
`agentbox stop` stops it, Podman removes the image if no other container
prevents removal. `agentbox` does not run separate image-pruning commands.

Named runtime cache volume lifecycle remains separate. `agentbox stop` leaves
the workspace cache volume intact so later sessions can reuse it. Volume
reclamation is explicit, for example `podman volume rm <container-name>` or
`podman volume prune --all`.

Required drift behavior:

- Duplicate containers for one git root: mark the session as `duplicate`, fail
  `run` and `attach`, and do not guess which container to use. `stop --force`
  may stop all duplicate managed containers that exactly claim the resolved
  canonical git root or exact orphan path.
- Missing or malformed managed-container metadata: mark the session as `failed`
  and require explicit cleanup or recreation before the session can be used
  again.
- Missing runtime cache volume mount for an existing session: fail clearly and
  require explicit container recreation.
- Missing or inconsistent attach endpoint metadata or published port data: mark
  the session as `failed` and require explicit cleanup or recreation before the
  session can be attached.
- Missing host-attached Nix prerequisite: fail clearly, report the missing
  mount, client, socket, config, or state-path requirement, and do not attempt
  to synthesize a bundled Nix installation.
- Runtime image setup failure: fail clearly and preserve inspectable runtime
  state when Podman has not already removed the container.
- Identity collision between different canonical git roots: fail clearly and do
  not treat them as the same workspace.
- Stop failure: report exactly which managed containers are still running or
  still inspectable.
- A `failed` session is not attachable. If enough metadata remains to identify
  it by git root or exact orphan path, `agentbox stop --force <directory>` may
  stop it. If the session cannot be matched safely, `ls` reports the concrete
  container name and the user must remove that container with Podman before
  starting a new session for the affected workspace.

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
- identity collision between different canonical git roots
- missing required managed-container metadata on an existing session
- concurrent lifecycle operation that cannot complete safely
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
