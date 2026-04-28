# Agentbox Specification

## Summary

`agentbox` is a Rust CLI for running code agents inside isolated Podman containers.

The MVP is workspace-centric rather than name-centric:

- `agentbox run <directory>`
- `agentbox attach <directory>`
- `agentbox ls`
- `agentbox stop <directory>`

`agentbox run <directory>` resolves `<directory>` to its canonical git root and launches a new managed workspace session for that repository as a foreground `podman run --rm`, with the runtime's actual agent command as the container main process and interactive stdio inherited directly. `agentbox` owns cleanup of managed containers, while images and named cache volumes remain explicit cache resources. If a matching managed session is already running for that repository, `run` fails clearly and directs the user to `agentbox attach <directory>` or `agentbox stop <directory>`.

MVP runtime support is OpenCode only. Codex remains future-facing and informative, not normative MVP scope.

The MVP runtime contract uses host-attached Nix at runtime. The container consumes the host's `/nix`, host `nix` client, and host `/etc/nix` configuration, and uses a Podman-managed named cache volume at `/home/user/.cache/nix`. The cache volume name is the same as the container name.

## Goals

- Run code agents in isolated, reproducible Linux environments.
- Make the primary user model "attach to this repo" rather than "remember this agent name".
- Keep durable state minimal: the workspace bind mount, one Podman-managed runtime cache volume, and flat container labels. Host-side coordination is limited to a per-root lock.
- Run the runtime's actual foreground agent command directly instead of maintaining a synthetic keepalive container layer.
- Manage lifecycle directly through the Podman CLI.
- Keep container cleanup under `agentbox` control while leaving image and named volume reclamation to explicit user or administrator action.

## Non-Goals

- Supporting macOS or Windows in the MVP.
- Managing more than one agent container for the same canonical git root.
- Silent runtime switching or auto-recreation on runtime mismatch.
- Depending on a Podman socket, daemon API, or external service manager.
- Acting as a generic container orchestrator.
- Persisting extra runtime state volumes beyond the workspace bind mount and one Podman-managed runtime cache volume.
- Maintaining host-side session metadata beyond the per-root lock.
- Bundling a standalone Nix installation inside the image.
- Automatically removing runtime images after each session.

## Assumptions

- Host OS is Linux.
- Supported host models are NixOS and Linux with multi-user Nix.
- Podman is available on the host.
- Git is available on the host.
- The target directory is expected to live inside a git repository.
- The runtime can be launched as the container main process in the target working directory.
- Host-attached Nix prerequisites are available when the OpenCode container starts.

## Core Model

### WorkspaceSession

A `WorkspaceSession` is the single managed agent environment for one canonical git root.

It has:

- one canonical git root absolute path
- one runtime type, `opencode` in MVP
- at most one managed Podman container at a time for that git root
- one bind mount of the git root at the same absolute path inside the container
- one Podman-managed named runtime cache volume mounted at `/home/user/.cache/nix`, with the same name as the container
- host-attached Nix inputs and runtime state as described below
- one per-git-root host lock used only for operation coordination

Normal MVP `run` launches that managed container in the foreground with `podman run --rm`. `agentbox` must not create an idle keepalive container whose only job is to stay alive for later `exec` or attach flows.

`run` does not pass `--rmi` by default. The runtime image is a reusable cache resource: the default image is built only when missing, and a user-supplied `--image` reference must not be removed implicitly by `agentbox`.

The canonical git root is the primary identity. There is exactly one managed container total per canonical git root.

The requested target directory is not part of identity. Different `run` or `attach` invocations may target different subdirectories under the same git root while still referring to the same running managed container.

### Runtime Adapter

Each runtime is described by an internal adapter with:

- container image reference
- foreground command template
- default environment variables
- whether `agentbox attach` can rely on generic `podman attach` for the running foreground process

Runtime differences stay inside the adapter layer. The generic CLI flow must not branch on runtime-specific details beyond the adapter contract.

## Identity And Path Resolution

`agentbox` resolves `<directory>` using the following rules:

1. Convert the user input to an absolute path.
2. Resolve the git root with `git -C <directory> rev-parse --show-toplevel`.
3. Canonicalize the resulting git root by resolving symlinks.
4. Canonicalize the requested target directory as well.
5. Require the target directory to remain inside the canonical git root.

Normative consequences:

- A non-git directory fails clearly. MVP does not create ad-hoc non-git sessions.
- A symlinked path resolves to the same canonical git root as the real path.
- Nested repositories use the git root reported for the requested target directory, so an inner repository gets its own session.
- Submodules and git worktrees each get their own session identity because each resolves to its own canonical git root.
- Moving a repository to a different absolute path creates a new identity. The old container becomes orphaned until explicitly removed.

## Naming

Each workspace session has a deterministic logical name derived from the canonical git root.

Normative algorithm:

1. Take the canonical git-root absolute path bytes.
2. Compute `SHA-256` of those bytes.
3. Use the first 12 lowercase hex characters of the digest as `hash12`.
4. Escape the canonical git-root path by replacing `/` with `_`.
5. Replace every other character outside `[A-Za-z0-9_.-]` with `-`.
6. Build the readable suffix from the escaped path by taking the full escaped path if it already fits, or else taking the rightmost characters of the escaped path so the final name fits within 63 characters including prefix, separator, and `hash12`.
7. Container names use the prefix `agentbox-`.
8. If the derived name is already occupied by a non-matching Podman object, fail with a name-conflict error. Do not generate an alternate name.

Required behavior:

- The escaped readable suffix remains visible in the final name.
- Overlong paths preserve the rightmost path segment characters, not the leftmost prefix characters.
- The exact same canonical git-root path always yields the exact same container name.
- The runtime cache volume name for a workspace session is exactly the same string as the container name.
- The algorithm does not depend on ambient Podman state to produce a different name.
- The 63-character maximum is owned by this spec for container names.

Example:

- canonical git root: `/aaa/bbb`
- escaped readable suffix: `_aaa_bbb`
- container name: `agentbox-_aaa_bbb-9ae5447864f7`
- runtime cache volume name: `agentbox-_aaa_bbb-9ae5447864f7`

The concrete naming algorithm must be deterministic from the canonical git root alone. Runtime is not part of identity because only one session per repository is allowed.

## CLI

### `agentbox run <directory>`

`run` launches a new workspace session in the foreground.

Expected behavior:

1. Resolve `<directory>` to a canonical git root and canonical target directory.
2. Acquire a per-git-root lock so concurrent `run` and `stop` operations cannot create duplicate containers or race teardown.
3. Discover existing managed containers for that canonical git root by label.
4. If more than one matching container exists, fail as `duplicate` and do not guess.
5. If exactly one matching container exists, fail clearly and suggest `agentbox attach <directory>` or `agentbox stop <directory>`.
6. If none exists, execute direct foreground `podman run --rm` with the required labels, mounts, image selection, and target-directory working directory.
7. The container main command must be the runtime's actual foreground agent command. For the OpenCode MVP this is `opencode` in the target working directory, optionally with an explicit project argument, rather than `sleep infinity`.
8. Interactive stdio is inherited directly by the foreground container process. `run` must not perform a separate detached-server start, readiness probe, or later attach step.

Optional flag:

- `--image <image>`

Image rules:

- `--image <reference>` selects the image reference for this foreground run of the sole MVP runtime.
- `agentbox` labels the running managed container with the exact image reference as `io.agentbox.image` so live discovery can report it while the container exists.
- If `--image` is supplied, use that exact image reference.
- If `--image` is omitted, use the runtime adapter's default image reference.
- `run` must not pass `--rmi` implicitly. Image cleanup is outside the `run` and `stop` lifecycle.
- If a managed session already exists for the resolved git root, `run` fails before reusing or comparing any stored image reference.
- `attach` does not accept or interpret `--image`.
- `--image` does not change session identity.

### `agentbox attach <directory>`

`attach` attaches to an already-running managed workspace session.

Expected behavior:

1. Resolve `<directory>` to a canonical git root and canonical target directory.
2. Acquire the same per-git-root lock used by `run`.
3. Discover the managed container by label.
4. Fail if the matching container is not running.
5. Attach to the running container's foreground process with stdio inherited.

Rules:

- `attach` must never create a new session.
- `attach` must never start or restart a stopped session.
- `attach` must never prompt for runtime selection.
- If no managed session exists for the resolved git root, fail clearly and suggest `agentbox run <directory>`.

### `agentbox ls`

`ls` lists managed workspace sessions.

Discovery is label-first:

- Query Podman for containers labeled as managed by `agentbox`.
- Derive status from required labels, `podman inspect`, and host path checks.

Expected output fields:

- canonical git root
- runtime
- status: `running`, `stopped`, `orphaned`, `duplicate`, or `failed`
- concrete container name

Status rules:

- `running`: container exists and is running.
- `stopped`: container exists but is not running.
- `orphaned`: stored git root path no longer exists on the host or no longer resolves as the same repository.
- `duplicate`: more than one managed container claims the same canonical git root.
- `failed`: container exists but required labels, inspectable mounts, or other inspectable session invariants are inconsistent.

### `agentbox stop <directory>`

`stop` stops the workspace session for the resolved repository and removes the managed container if it still exists. It is an idempotent cleanup command for managed containers, including stale or legacy leftovers, not an image or volume pruning command.

Expected behavior:

1. Resolve `<directory>` to the canonical git root.
2. If `<directory>` does not exist, allow an exact absolute git-root path string to match an orphaned session directly.
3. Acquire the per-git-root lock.
4. Stop the container if it is running.
5. Remove the container if it still exists, including stopped or legacy leftovers.
6. Leave runtime images unmanaged by `stop`.
7. Leave the runtime cache volume unmanaged by `stop` so it can be reclaimed later by explicit Podman volume cleanup.

Optional flags:

- `--force`: best-effort cleanup when duplicate exact matches exist

Safety rule:

- `stop` never deletes the user workspace.
- `stop` never removes images or named cache volumes.

## Completion

Shell completion for `attach` and `stop` is dynamic.

Required behavior:

- Completion candidates come from live Podman discovery, not from a static file.
- Candidate values are canonical git root paths.
- Candidate descriptions include runtime and status when the shell supports descriptions.
- Running sessions are visible immediately at tab completion time.
- `fzf-tab`-style frontends work automatically because they consume normal shell completion results.

Implementation guidance:

- A completion script uses live discovery from Podman labels and `podman inspect`.
- The MVP does not require a machine-readable `ls` mode.
- `agentbox ls` remains human-readable in MVP.

## Container And Filesystem Model

Each workspace session container has exactly these persistent mounts in the MVP:

- one bind mount of the canonical git root
- one Podman-managed named volume mounted at `/home/user/.cache/nix`, using the same name as the container
- host-attached Nix inputs and configuration mounts as described below

### Workspace Mount

The canonical git root is bind-mounted at the same absolute host path inside the container.

Example:

- host git root: `/aaa/bbb`
- container git root mount: `/aaa/bbb`

This same absolute path rule is required so file paths emitted by the agent match the host filesystem layout.

### Target Directory CWD

The effective working directory for a given `run` or `attach` invocation is the requested target directory, not always the git root.

Example:

- command: `agentbox run /aaa/bbb/subdir`
- mounted git root inside container: `/aaa/bbb`
- working directory seen by the runtime for that invocation: `/aaa/bbb/subdir`

### Runtime Cache Volume

Each workspace session has a writable runtime home at `/home/user`, but only `/home/user/.cache/nix` is backed by a Podman-managed named volume.

Rules:

- The runtime user home inside the container remains `/home/user`.
- `/home/user` itself is writable for runtime state creation, but it is not required to persist across container recreation.
- The runtime cache volume name is identical to the container name for the same workspace session.
- The mounted runtime cache volume stores Nix cache and evaluation artifacts that should survive container restarts.
- The active runtime profile does not live under the cache volume.
- The runtime profile default path is `$XDG_STATE_HOME/nix/profile`.
- If `XDG_STATE_HOME` is unset and `HOME` is set, the runtime falls back to `$HOME/.local/state/nix/profile`.
- If both `XDG_STATE_HOME` and `HOME` are unavailable, the runtime falls back to `/home/user/.local/state/nix/profile`.
- No other subpath under `/home/user` is required to persist in MVP.
- `agentbox stop <directory>` does not explicitly delete the runtime cache volume.
- Once no container uses the cache volume, it remains available for later explicit reclamation, for example with `podman volume rm <container-name>` or `podman volume prune --all`.

### Direnv

When the target directory uses `direnv`, the runtime command for that invocation is executed from the target directory context as:

- `direnv exec . <agent>`

Rules:

- `direnv` evaluation happens relative to the requested target directory, not forcibly at the git root.
- `direnv` wraps the runtime foreground command for `run`, not a later attach step.
- When `run` launches a session, `agentbox` evaluates `direnv exec . <agent>` from the requested target directory before starting the foreground runtime command.
- A running session keeps the environment from its original foreground start. `attach` to an already-running session does not reevaluate or replace that environment.
- The MVP does not persist host-side direnv state for running-session compatibility checks.
- `attach` to an already-running session reuses the running session as-is. MVP does not compare the requested target directory against earlier direnv contexts for that running session.
- If `.envrc` is present but `direnv` is unavailable, blocked, or fails to load, `run` fails clearly.
- If no `.envrc` applies, the runtime launches normally.

### Host-Attached Nix Model

The OpenCode runtime in MVP uses host-attached Nix support inside the container alongside a Podman-managed named Nix cache volume.

Supported host models:

- NixOS
- Linux with multi-user Nix

Default image asset source:

- The repository's canonical source of truth for the default runtime image assets is `assets/image/`.
- The installed `agentbox` binary embeds only the files required to assemble the default `podman build` context: `Containerfile`, `bootstrap`, `entrypoint`, `lib/runtime-contract.sh`, and `runtime-packages.nix`.
- When `agentbox run` needs to build the default image, it materializes those embedded files into a temporary readable build context and invokes `podman build` from that temporary directory.

Rules:

- `/nix` is mounted into the container so the host Nix store and nix-daemon socket are available.
- `NIX_REMOTE=daemon` and the daemon socket at `/nix/var/nix/daemon-socket/socket` are part of the runtime contract.
- A host `nix` client is available in `PATH`, commonly mounted at `/usr/local/bin/nix`.
- `/etc/nix` is mounted so host configuration and registry inheritance are visible inside the container.
- `/etc/static/nix` is mounted only when needed because `/etc/nix` resolves there on the host model.
- The image startup path validates these host-attached prerequisites, uses the cache volume for Nix cache artifacts, materializes runtime packages from `runtime-packages.nix` with `nix profile add`, and then hands off to `/entrypoint`.
- Runtime profile state lives under `$XDG_STATE_HOME/nix/profile`, with fallback to `$HOME/.local/state/nix/profile` or `/home/user/.local/state/nix/profile` when needed.
- The image provides its own CA bundle. Host SSL trust-store mounts are out of scope for MVP.
- The supported later exec contract is `/entrypoint <cmd>`.

A Podman-managed named Nix cache volume is created in the MVP. Its name is identical to the workspace session container name.

## Labels And Locking

Discovery is label-first.

Required container labels:

- `io.agentbox.managed=true`
- `io.agentbox.schema=1`
- `io.agentbox.git_root=<canonical git root>`
- `io.agentbox.git_root_hash=<hash12>`
- `io.agentbox.runtime=<runtime>`
- `io.agentbox.image=<exact stored image reference>`
- `io.agentbox.logical_name=<logical readable name>`

Normative rule:

- The required labels above are the complete live session record in MVP while the managed container exists.
- The full canonical git root is authoritative for identity. `io.agentbox.git_root_hash` is an index only and must never be trusted without verifying the exact `io.agentbox.git_root` value.

### Per-Root Lock

Mutating operations use one host-side lock per canonical git root.

Example paths:

- `$XDG_STATE_HOME/agentbox/locks/<git-root-digest>.lock`

Where `git-root-digest` is the full 64-character lowercase hex `SHA-256` of the canonical git-root path.

Rules:

- The per-root lock is host-side coordination only. It is not session metadata and is never used for discovery.
- The Podman-managed runtime cache volume mounted at `/home/user/.cache/nix` is persistent runtime state and is not represented as a host-side metadata path.

Hash collision rules:

- Short-hash label lookup is permitted only as a prefilter.
- Any container or lock entry selected by hash must be verified against the full canonical git root before it is treated as a match.
- If two different canonical git roots collide on `hash12`, fail clearly with a hash-collision error rather than aliasing discovery or locking.

`podman inspect` is authoritative for container existence, running state, labels, mounts, and published port mappings.

Supported operational behavior in MVP must be derivable from required labels and `podman inspect` alone.

## Lifecycle Management

All lifecycle operations use the Podman CLI through direct process invocation. MVP is Podman CLI-only.

`agentbox` must not require a Podman socket or API service.

High-level command strategy:

1. `run` creates the workspace session as a foreground `podman run --rm`. If a matching session already exists, it fails clearly and suggests `attach` or `stop`.
2. `attach` discovers an existing running workspace session and attaches to its main process.
3. `ls` derives session status from live Podman state and host path checks.
4. `stop` stops the container and removes it if it still exists.

Image lifecycle is separate from session lifecycle. The default runtime image is treated as a reusable local cache and is built only when absent. `agentbox run` must not use `--rmi` by default, and `agentbox stop` must not remove images. Image reclamation is an explicit user or administrator operation, for example `podman image prune` or `podman rmi <image>`.

Named runtime cache volume lifecycle is also separate. `agentbox stop` leaves the workspace cache volume intact so later sessions can reuse it. Volume reclamation is explicit, for example `podman volume rm <container-name>` or `podman volume prune --all`.

### `run` Sequence

Required sequence:

1. Validate Podman, Git, runtime prerequisites, and the host-attached Nix contract.
2. Resolve the canonical git root and target directory.
3. Acquire the per-root lock.
4. Query Podman for managed containers with the matching git-root hash, then verify the exact `io.agentbox.git_root` label matches the resolved canonical git root before treating a container as a candidate.
5. If more than one container matches, fail as `duplicate` and do not guess.
6. If one matching managed session already exists, fail clearly and suggest `agentbox attach <directory>` or `agentbox stop <directory>`.
7. If none matches, execute `podman run --rm` with the resolved container name, required labels, required mounts, target-directory working directory, and the runtime foreground command as the container main command. Do not pass `--rmi` by default.
8. The image bootstrap path validates prerequisites, materializes the runtime profile, hands off to `/entrypoint`, and then execs the requested foreground runtime command.
9. Inherit interactive stdio directly for the foreground run.

### `attach` Sequence

Required sequence:

1. Resolve the canonical git root and target directory.
2. Acquire the per-root lock.
3. Query Podman for managed containers with the matching git-root hash, then verify the exact `io.agentbox.git_root` label matches the resolved canonical git root before treating a container as a candidate.
4. Fail if none exists.
5. Fail if duplicates exist.
6. Fail if the matching container is not running.
7. Execute `podman attach` against the running managed container.

### `ls` Sequence

Required sequence:

1. Query Podman for all managed containers.
2. Group by full canonical git root, using the short hash only as a discovery index.
3. Use `podman inspect` and host path checks to mark duplicates, orphaned paths, stopped containers, and inspectable failures.
4. Print a compact table in the default view.
5. Do not require machine-readable output in MVP.

### `stop` Sequence

Required sequence:

1. Resolve the target to a canonical git root or direct orphaned git-root identity.
2. Acquire the per-root lock.
3. Stop the container as best effort.
4. Remove the container if it still exists.
5. Treat an already-removed container as success after verifying it is absent.
6. Leave the runtime image available for reuse.
7. Leave any now-unused named cache volume available for later explicit reclamation, for example with `podman volume rm <container-name>` or `podman volume prune --all`.
8. Surface any partial failure with enough detail for manual managed-container cleanup.

## Interactive Transport

`agentbox run` uses the runtime's own foreground process directly.

The supported MVP `run` path is direct foreground `podman run --rm` with inherited stdio.

The supported MVP `attach` path is `podman attach` to an already-running managed container.

Rules:

- The generic CLI must not assume a detached runtime-native server/client attach contract for MVP `run`.
- The generic CLI must not assume raw `podman exec ... zsh` is a supported contract.
- `/entrypoint` re-establishes `NIX_SSL_CERT_FILE`, `NIX_PROFILES`, `PATH`, and `XDG_DATA_DIRS` before exec.
- Host-published ports are avoided unless a runtime requires them.
- If a runtime needs a published port, the adapter must make the endpoint discoverable from scalar labels or `podman inspect`.
- Do not embed attach configuration as a large JSON label.

## Error Handling And Drift Recovery

The CLI must produce actionable errors that say what failed, which workspace was involved, which external command failed when relevant, and what the user can try next.

Required cases:

- non-git target directory
- requested directory escapes the resolved git root
- unsupported runtime
- Podman not installed
- Git not installed
- runtime mismatch
- container failed to start
- foreground runtime command not found
- attach failed
- duplicate managed containers for one git root
- `run` called for a git root that already has a managed session
- hash collision between different canonical git roots
- missing required label on an existing session
- stale lock file with no live owner
- missing runtime cache volume mount for an existing session
- orphaned session after repo move
- missing host `nix` client in `PATH`
- missing nix-daemon socket at `/nix/var/nix/daemon-socket/socket`
- missing `/etc/nix` host mount or unreadable `/etc/nix/nix.conf`
- missing readable `/etc/static/nix` target when `/etc/nix` resolves there
- missing image-local CA bundle
- unusable runtime profile path under the XDG state or HOME fallback location
- `/entrypoint` contract failure

Required drift behavior:

- Stale lock file with no live owner: clear the stale lock automatically before proceeding.
- Duplicate containers for one git root: mark the session as `duplicate`, fail `run` and `attach`, and do not guess which container to use.
- Missing required label on an existing session: mark the session as `failed` and require explicit repair or recreation before the session can be used again.
- Missing runtime cache volume mount for an existing session: fail clearly and require explicit container recreation.
- Missing host-attached Nix prerequisite: fail clearly, report the missing mount, client, socket, config, or state-path requirement, and do not attempt to synthesize a bundled Nix installation.
- Missing image-local CA bundle: fail clearly and require an image fix.
- `/entrypoint` contract failure: fail clearly and preserve the runtime state for inspection.
- Hash collision between different canonical git roots: fail clearly and do not alias discovery or locking.
- Partial cleanup failure during `stop`: report exactly which managed containers remain.

## Security And Isolation

MVP isolation expectations:

- separate rootless Podman container per workspace session
- explicit workspace mount only for the canonical git root
- host-provided Nix inputs mounted alongside one Podman-managed cache volume
- one writable Podman-managed named cache volume at `/home/user/.cache/nix`
- minimal privileges
- networking disabled unless the runtime adapter requires network access or a published attach endpoint

Runtime user and bind-mount rules:

- The container runs as the non-root `user` account with home `/home/user`.
- The workspace bind mount is read-write by default.
- The Podman-managed runtime cache volume at `/home/user/.cache/nix` is writable by the runtime user.
- Host ownership and permission bits remain authoritative.
- `agentbox` must not `chown`, `chmod`, remount, or elevate privileges to force access.
- If the runtime user cannot read or write a required path inside the bind mount, `run` or `attach` fails clearly with the affected path and the permission problem.
- `agentbox` must not repair host workspace permissions by mutating the host mount.
- `agentbox` must not repair host Nix access by mutating host permissions or host configuration.

Out of scope for MVP:

- hardened sandbox guarantees beyond normal rootless Podman isolation
- secret brokering or policy-based filesystem mediation
- cross-host or multi-user orchestration

## Implementation Notes

Informative only, not normative.

Example internal modules:

- `cli`: Clap definitions and argument parsing
- `workspace`: canonical path resolution and identity
- `runtime`: runtime adapters for `codex` and `opencode`
- `container`: container lifecycle abstraction
- `podman`: Podman CLI backend
- `lock`: per-root host locking
- `completion`: dynamic shell completion helpers
- `error`: shared error types

Guiding principles:

- Prefer small, explicit Podman CLI invocations over a heavier container API dependency.
- Keep container metadata flat and queryable.
- Treat the container as mostly stateless. Persist only the workspace mount, the Podman-managed runtime cache volume, and the required container labels.
- Keep host-side coordination minimal: per-root locking only.

## First Milestone

Informative only, not normative.

First milestone focus:

1. Support Linux only.
2. Support OpenCode end-to-end first.
3. Implement canonical git root resolution and deterministic naming.
4. Implement `run <directory>`, `attach <directory>`, `ls`, and `stop <directory>`.
5. Implement same-path workspace bind mounts and target-directory cwd behavior.
6. Implement the host-attached Nix runtime contract with `/nix`, `/etc/nix`, `/entrypoint`, and a Podman-managed cache volume at `/home/user/.cache/nix`.
7. Implement label-first discovery, `podman inspect`-derived session state, and per-root locking.
8. Implement dynamic completion for live sessions.

## Future Work

Informative only, not normative.

- Codex support as a future runtime adapter.
- Machine-readable `ls` output.
- Additional runtime adapters.
- Broader attach transport options.
