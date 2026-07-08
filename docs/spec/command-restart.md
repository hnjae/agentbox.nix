# `agentbox restart [--connect|-c] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [target]`

`restart` replaces one running managed workspace session for the same canonical git root. It preserves the selected runtime, deterministic container name, stored launch directory, stored agent server arguments, stored resource limits, and named runtime cache volume, while re-evaluating the server command's development environment.

Optional flags and arguments:

- `--connect`, `-c`: connect with the runtime host-side client after the new session is ready
- `--dev-env <auto|none>`: controls automatic development environment wrapping for the replacement runtime server. The default is `auto`.
- `--cpus <cpus>`: sets a non-negative decimal CPU limit for the replacement server container. `0` disables the CPU limit and replaces the stored value.
- `--memory <memory>`: sets a Podman memory limit for the replacement server container. `0` disables the memory limit and replaces the stored value.
- `[target]`: workspace directory, exact stored git-root absolute path, or stable session id prefix

Expected behavior:

1. If `[target]` is omitted and stdin and stderr are terminals, discover restartable running managed sessions and prompt on stderr with a fuzzy single-select list.
2. If `[target]` is omitted and either stdin or stderr is not a terminal, fail with a clear error that a restart target is required in non-interactive use.
3. If `[target]` names an existing path, resolve it to a canonical git root.
4. If `[target]` is an absolute path that does not exist, require it to exactly match a live running managed session's stored `io.agentbox.git_root` absolute path string.
5. If `[target]` is not resolved as a path, treat it as a case-insensitive stable id prefix.
6. Select exactly one running managed session. A transient `run` container, stopped session, orphaned session, failed session, duplicate session, malformed runtime or launch-directory label, or missing target match is a clear failure and is not restarted.
7. Lock the selected canonical git root and re-discover the target before any lifecycle mutation. If the selected target is no longer exactly one running managed session, fail before stopping anything.
8. Read the runtime, stored launch directory, stored agent server arguments, and stored resource limits from the existing managed session. The requested target identifies the session only; it does not update the launch directory or stored defaults unless a new resource-limit flag is supplied.
9. Validate Podman, the recovered runtime, and host-attached Nix prerequisites.
10. Resolve the development environment from the stored launch directory.
11. If `--connect` is present, verify that the recovered runtime's host client command is available before stopping the existing container.
12. Ensure the recovered runtime's default image is available before stopping the existing container.
13. Stop the existing managed container and verify that it is gone. If stop or cleanup verification fails, do not start a replacement container and report the remaining container.
14. Start detached `podman run --detach --rm` with the same deterministic container name, selected runtime, stored launch directory as the container working directory, required labels and mounts, default runtime image, local-only published attach endpoint, existing named runtime cache volume, and stored agent server arguments.
15. Wait until the replacement runtime server endpoint is ready for connection or the container exits.
16. If `--connect` is absent, report that the replacement session is ready and suggest `agentbox connect <launch-directory>`.
17. If `--connect` is present, report that the replacement session is ready and execute the runtime host client command from the stored launch directory with stdio inherited, using the same client command behavior as `agentbox connect`.

Progress and diagnostics:

- `restart` prints short `INFO` log progress to stderr while checking prerequisites, resolving the target, ensuring the runtime image, stopping the old container, starting the replacement container, and waiting up to 90 seconds for the runtime server endpoint.
- `restart` prints its final success message as an `INFO` log on stderr. Successful `restart` does not write to stdout.
- With `--verbose`, `restart` also prints the external commands it executes and forwards non-JSON Podman command output as `DEBUG` logs on stderr.
- If replacement container startup or readiness fails after the old container was stopped, `restart` reports that the previous session may already be gone and includes a short `podman logs --tail` excerpt for the replacement managed container when Podman can provide one.

Runtime rules:

- `restart` does not accept `--runtime`; the runtime is recovered from the existing session metadata.
- `restart` does not accept new agent passthrough arguments. It reuses the stored server arguments from the managed session being replaced.
- If stored agent server argument or resource-limit metadata is malformed, `restart` fails before stopping the existing session.
- `restart` does not accept `--all` or `--force`.
- `--dev-env auto` is the default and re-evaluates automatic development environment loading for the replacement server command from the stored launch directory.
- `--dev-env none` disables automatic development environment loading for the replacement server command.
- `--connect` does not change session identity, runtime recovery, container startup, endpoint readiness checks, or target selection.
- Transient `run` containers are never restartable.
- Stopped, failed, orphaned, and duplicate resources are not restartable. They require explicit `agentbox stop` cleanup followed by `agentbox start` if a new session is desired.
- Restart is not all-or-nothing. After the old container is stopped, a later replacement start or readiness failure can leave no running managed session.
