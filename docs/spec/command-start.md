# `agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-server-args>...]`

`start` launches a new workspace session as a detached runtime server.

Optional flags:

- `--connect`, `-c`: connect with the runtime host-side client after the new session is ready
- `--runtime <opencode|codex>`
- `--dev-env <auto|none>`: controls automatic development environment wrapping for the runtime server. The default is `auto`.
- `--cpus <cpus>`: sets a non-negative decimal CPU limit for the managed server container. `0` disables the CPU limit and is preserved for later `restart`.
- `--memory <memory>`: sets a Podman memory limit for the managed server container. `0` disables the memory limit and is preserved for later `restart`.
- `<agent-server-args>...`: arguments passed to the selected runtime server command. They must appear after a `--` delimiter so `agentbox` flags and server flags are unambiguous. With `--connect`, these arguments remain server arguments and are not passed to the host client.

Expected behavior:

1. If `--runtime` is omitted and stdin and stderr are terminals, prompt on stderr with a fuzzy single-select list of supported runtimes sorted by runtime name. Use the selected runtime exactly as if the user had passed `--runtime`.
2. If `--runtime` is omitted and either stdin or stderr is not a terminal, fail before workspace or runtime validation with a clear error that `--runtime` is required in non-interactive use.
3. Validate Git availability and resolve `[directory]` to a canonical git root and canonical target directory. If `[directory]` is omitted, resolve `.`.
4. Validate Podman, the selected runtime, and the host-attached Nix prerequisites.
5. Ensure concurrent lifecycle operations for the same git root do not leave duplicate sessions or ambiguous lifecycle state.
6. Discover existing managed sessions and transient `run` containers for that canonical git root.
7. If more than one matching agentbox container exists, fail as `duplicate` and do not guess which one to use.
8. If exactly one matching agentbox container exists, fail clearly instead of reusing or replacing it. For a healthy running managed session, suggest `agentbox connect <directory>` or `agentbox stop <directory>`. For a transient `run`, suggest `agentbox stop <id>`.
9. If none exists, record the canonical target directory as the session launch directory and start detached `podman run --detach --rm` with the required labels, including `io.agentbox.managed=true` and `io.agentbox.container_kind=managed-session`, mounts, default runtime image, local-only published attach endpoint, and launch-directory working directory.
10. Start the selected runtime server for the session, followed by any `<agent-server-args>`, and record those arguments in managed-session metadata. With the default `--dev-env auto`, the server starts through the selected development environment wrapper, if one applies. With `--dev-env none`, the server command starts directly.
11. Wait until the runtime server endpoint is ready for connection or the container exits.
12. If `--connect` is absent, report that the discovered attach endpoint is ready and suggest `agentbox connect <directory>`.
13. If `--connect` is present, report that the discovered attach endpoint is ready and execute the runtime host client command from the session's launch directory with stdio inherited, using the same client command behavior as `agentbox connect`, without passing `<agent-server-args>` to the host client.

Progress and diagnostics:

- `start` prints short `INFO` log progress to stderr while checking prerequisites, resolving session state, ensuring the runtime image, starting the detached container, and waiting for the runtime server endpoint.
- `start` prints its final success message as an `INFO` log on stderr. Successful `start` does not write to stdout.
- With `--verbose`, `start` also prints the external commands it executes and forwards non-JSON Podman command output as `DEBUG` logs on stderr.
- If the runtime container fails to start, exits before readiness, or times out after the 90-second readiness deadline before becoming reachable, `start` includes a short `podman logs --tail` excerpt for the managed container when Podman can provide one.

Runtime rules:

- `start` accepts only `opencode` and `codex`.
- `--runtime` selects the runtime for the new session when it is present.
- `--dev-env auto` is the default and enables automatic development environment loading for the server command.
- `--dev-env none` disables automatic development environment loading for the server command.
- Runtime server passthrough options are accepted only after the `--` delimiter. Without the delimiter, clap rejects them as `agentbox start` options.
- Stored agent server arguments are reused by `restart`. If the stored metadata is malformed, `restart` fails before stopping the existing session.
- `--connect` does not change session identity, runtime selection, container startup, endpoint readiness checks, or existing-session handling.
- If `--connect` is set and a managed session or transient `run` already exists for the resolved git root, `start` still fails before reusing or connecting to that resource.
- When `--runtime` is absent in an interactive terminal, the runtime prompt is rendered on stderr and the final success message is an `INFO` log on stderr.
- Canceling the runtime prompt with Escape exits non-zero with `selection canceled`.
- Interrupting the runtime prompt with Ctrl-C exits non-zero with `selection interrupted`.
- After container launch has started, interrupting `start` with Ctrl-C before the final success message exits non-zero and triggers best-effort cleanup for resources created by that `start` invocation.
- Ctrl-C cleanup attempts to stop and remove the managed container created by the interrupted `start`.
- Ctrl-C cleanup removes the workspace cache volume only when that volume did not exist before the interrupted container launch.
- Ctrl-C cleanup does not remove the selected runtime's default image.
- If Ctrl-C cleanup cannot fully stop the container or remove an eligible cache volume, `start` reports a partial cleanup warning or error.
- If a managed session or transient `run` already exists for the resolved git root, `start` fails before reusing or comparing any stored runtime value.
- `--runtime` does not change session identity.
- If the host client launched by `start --connect` exits unsuccessfully or cannot be started, `start --connect` exits non-zero and reports that the managed session remains running so the user can retry `agentbox connect <directory>` or stop it with `agentbox stop <directory>`.
