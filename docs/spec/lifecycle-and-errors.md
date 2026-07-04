# Lifecycle And Errors

## Lifecycle And Drift Recovery

Valid lifecycle behavior:

- `run` creates a transient runtime server container, connects with the selected host client, stops the transient container before exiting, and does not create a managed session.
- `start` creates the workspace session as a detached runtime server container.
- `restart` stops one running managed session and creates its replacement as a detached runtime server container for the same runtime and launch directory.
- `connect` discovers an existing running workspace session and runs the runtime host client against its published endpoint.
- `ps` derives agentbox container status from live Podman state and host path checks.
- `stop` stops the container and relies on the container's `--rm` run option for container cleanup.
- Concurrent lifecycle operations for the same canonical git root do not leave more than one valid agentbox runtime-server container or ambiguous cleanup outcome.

Default runtime image lifecycle is separate from managed sessions. When a foreground `exec` exits, when a transient `run` container is stopped, when a managed runtime server exits, when `agentbox stop` stops a managed session or transient `run` container, or when `agentbox restart` stops the old managed session, Podman removes the container but keeps the default runtime image. Current default runtime images are tagged by embedded build-context content hash. Runtime image package updates happen through `agentbox runtime update <opencode|codex|--all|-a>`, and unused agentbox-owned default runtime images can be removed through `agentbox clean`; `stop` does not remove or rebuild images.

Named runtime cache volume lifecycle remains separate. Transient `run`, foreground `exec`, `agentbox stop`, and `agentbox restart` leave the workspace cache volume intact so later one-shot runs or detached sessions can reuse it. Volume reclamation is explicit through `agentbox clean` or direct Podman commands.

Required drift behavior:

- Duplicate agentbox containers for one git root: mark the resources as `duplicate`, fail `run`, `start`, and `restart`, fail `connect` when duplicate managed sessions exist, and do not guess which container to use. `stop --force` may stop all duplicate agentbox containers that exactly claim the resolved canonical git root, exact stored git-root path, or selected stable id.
- Missing or malformed agentbox-container metadata: mark the resource as `failed` and require explicit cleanup or recreation before it can be used again.
- Missing runtime cache volume mount for an existing session, including a bind mount where the named volume is expected: fail clearly and require explicit container recreation.
- Missing or inconsistent attach endpoint metadata or published port data: mark the session as `failed` and require explicit cleanup or recreation before the session can be connected.
- Missing host-attached Nix prerequisite: fail clearly, report the missing mount, client, socket, config, or state-path requirement, and do not attempt to synthesize a bundled Nix installation.
- Runtime image setup failure: fail clearly and preserve inspectable runtime state when Podman has not already removed the container.
- Container readiness and discovery diagnostics must inspect Podman containers specifically, so a same-named runtime cache volume never masks a missing or auto-removed container as malformed container JSON.
- Identity collision between different canonical git roots: fail clearly and do not treat them as the same workspace.
- Stop failure: report exactly which managed containers are still running or still inspectable.
- Restart stop failure: report the old managed container that is still inspectable and do not start a replacement container.
- Restart replacement failure after the old container is stopped: report that the previous managed session may already be gone and include replacement container logs when available.
- A `failed` session is not connectable. If enough metadata remains to identify it by git root, exact stored git-root path, or stable id, `agentbox stop --force <target>` may stop it. If the session cannot be matched safely, `ps` reports the concrete container name and the user must remove that container with Podman before starting a new session for the affected workspace.

## Error Handling

The CLI must produce actionable errors that say what failed, which workspace was involved, which external command failed when relevant, and what the user can try next.

Required error cases:

- non-git target directory
- requested directory escapes the resolved git root
- unsupported runtime
- unsupported `run --connect` or `run -c`
- unsupported `restart --runtime`, `restart --all`, or `restart --force`
- Podman not installed
- Git not installed
- unsupported or malformed runtime metadata on an existing managed session
- malformed stored resource-limit metadata on an existing managed session
- container failed to start
- container failed to become reachable within the 90-second readiness timeout
- runtime server command not found
- runtime host client command not found
- connect failed
- missing or inconsistent attach endpoint metadata
- missing published attach port
- duplicate managed containers for one git root
- `run` or `start` called for a git root that already has a managed session
- `restart` target does not resolve to exactly one running managed session
- `restart` target resolves to a transient `run`, stopped session, failed session, orphaned session, duplicate session, malformed runtime label, or malformed launch-directory label
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
- missing or unusable host `${HOME}/.codex` for `run --runtime codex`, `start --runtime codex`, or `restart` of a Codex session
- missing or unusable host OpenCode configuration or data directories for `run --runtime opencode`, `start --runtime opencode`, or `restart` of an OpenCode session
- missing host `npm` when a runtime npm version must be resolved
- unusable runtime profile path under the XDG state or HOME fallback location
- runtime image setup failure
- selected development environment wrapper unavailable, blocked, or failing during runtime process startup
- automatic flake evaluation failure while resolving a `nix develop` wrapper
- workspace or host Nix permission problems that prevent required access
- `clean` run from non-TTY stdin without `--yes` or `--dry-run`
- partial `clean` deletion failures
