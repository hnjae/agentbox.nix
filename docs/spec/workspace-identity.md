# Workspace Identity

## Workspace Identity And Path Resolution

A workspace session is the single valid managed agent environment for one canonical git root.

`agentbox` resolves `<directory>` using these rules:

1. Convert the user input to an absolute path.
2. Resolve the git root with `git -C <directory> rev-parse --show-toplevel`.
3. Canonicalize the resulting git root by resolving symlinks.
4. Canonicalize the target directory as well.
5. Require the target directory to remain inside the canonical git root.

After resolution, "target directory" means this canonical target directory, not the raw path spelling entered by the user.

When `start` successfully launches a session, the canonical target directory becomes the session's launch directory. The launch directory is recorded with the session and remains the stable working-directory for later connects to that running session. Transient `run` uses the canonical target directory as the server container and host-client working directory, but it does not record session metadata.

Required behavior:

- A symlinked path resolves to the same canonical git root as the real path.
- Nested repositories use the git root reported for the target directory, so an inner repository gets its own session.
- Submodules and git worktrees each get their own session identity because each resolves to its own canonical git root.
- Moving a repository to a different absolute path creates a new identity.
- A still-running container whose stored git-root path no longer exists is reported as `orphaned` until it is stopped.
- The target directory is not part of identity. A `start` invocation may choose any subdirectory under the git root as the launch directory for a new session, and a `connect` invocation may provide any subdirectory under the same git root to find that session. A transient `run` invocation may also choose any subdirectory under the git root as its one-off server and host-client working directory.
- `connect` target directories do not retarget a running session. They identify the workspace session, then the running session's stored launch directory controls host-client working directory.

## Naming And Visible Podman State

Each workspace session has a deterministic logical name derived from the canonical git root.

Name contract:

- Container names use the prefix `agentbox-`.
- The final name includes a readable suffix derived from the canonical git root.
- Overlong paths preserve the rightmost path segment characters, not the leftmost prefix characters.
- The same canonical git-root path always yields the same container name.
- Runtime is not part of identity because only one session per repository is allowed.
- The runtime cache volume name matches the workspace session container name.
- Ambient Podman state does not cause the same canonical git root to produce a different name.
- The 63-character maximum is owned by this spec for managed container names.
- If the derived name is already occupied by a non-matching Podman object, fail with a name-conflict error. Do not generate an alternate name.
- If two different canonical git roots would produce indistinguishable managed identities, fail clearly with an identity-collision error rather than treating them as the same workspace.

Example shape:

- canonical git root: `/aaa/bbb`
- readable suffix: `_aaa_bbb`
- container name starts with `agentbox-_aaa_bbb-`
- runtime cache volume name matches the container name

Managed containers are visible through Podman while they are running. They carry Podman labels that identify at least:

- that the container is managed by `agentbox`
- that the container kind is `managed-session`
- the metadata schema version
- the canonical git root
- a stable git-root identity token
- the selected runtime
- the default runtime image reference used for the session
- the canonical session launch directory
- the logical name
- the attach endpoint scheme
- the runtime server container port and listen address
- the stored agent server arguments in `io.agentbox.server_args`, when any were supplied at `start`
- the resolved CPU resource limit in `io.agentbox.resource_limits.cpus`, with `0` stored explicitly for unlimited
- the resolved memory resource limit in `io.agentbox.resource_limits.memory`, with `0` stored explicitly for unlimited

`agentbox` discovers sessions from live Podman state. It does not require a separate host-side session database.

Transient `run` containers are not managed containers. They use the workspace's deterministic runtime cache volume name and publish a local-only attach endpoint for the matching host client. They omit the managed-session marker label, but carry `io.agentbox.container_kind=transient-run` plus canonical git root, stable git-root identity token, selected runtime, default runtime image reference, launch directory, logical name, attach endpoint scheme, and runtime server port/listen address. `run` client arguments are not recorded on transient containers. `ps` and `stop` discover transient `run` containers; `connect` and `health` do not.

`agentbox` treats a live container as agentbox-owned only when it carries either `io.agentbox.managed=true` or `io.agentbox.container_kind=transient-run`. Image labels, image references, and container name patterns alone do not prove ownership.

During live Podman discovery, missing or `null` JSON collection fields such as container labels are treated as empty collections. Ambient containers without agentbox ownership labels are ignored and must not make lifecycle commands fail.

When a command is scoped to one canonical git root, containers that advertise a different git-root identity token are outside that command's discovery scope and must not block it. Containers with a missing identity token remain in scope until full inspection proves whether they match.

When a deterministic container-name conflict is inspected, `agentbox` uses the conflicting managed container's recoverable workspace identity labels to report the conflicting git root before evaluating runtime-specific attach metadata. Malformed runtime or attach metadata on a managed container for a different git root must not hide that the failure is a different-workspace name conflict.
