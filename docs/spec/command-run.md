# `agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-client-args>...]`

`run` launches a transient selected-runtime server container, waits until its local-only attach endpoint is ready, and connects with the selected runtime host client.

Optional flags:

- `--runtime <opencode|codex>`
- `--dev-env <auto|none>`: controls automatic development environment wrapping for the transient runtime server. The default is `auto`.
- `--cpus <cpus>`: sets a non-negative decimal CPU limit for the transient server container. `0` disables the CPU limit.
- `--memory <memory>`: sets a Podman memory limit for the transient server container. `0` disables the memory limit.
- `<agent-client-args>...`: arguments passed to the selected runtime host client. They must appear after a `--` delimiter so `agentbox` flags and client flags are unambiguous.

Expected behavior:

1. If `--runtime` is omitted and stdin and stderr are terminals, prompt on stderr with a fuzzy single-select list of supported runtimes sorted by runtime name. Use the selected runtime exactly as if the user had passed `--runtime`.
2. If `--runtime` is omitted and either stdin or stderr is not a terminal, fail before workspace or runtime validation with a clear error that `--runtime` is required in non-interactive use.
3. Validate Git availability and resolve `[directory]` to a canonical git root and canonical target directory. If `[directory]` is omitted, resolve `.`.
4. Validate Podman, the selected runtime, the selected runtime host client command, and the host-attached Nix prerequisites.
5. Ensure concurrent lifecycle operations for the same git root do not leave duplicate sessions or ambiguous lifecycle state.
6. Discover existing managed sessions and transient `run` containers for that canonical git root.
7. If more than one matching agentbox container exists, fail as `duplicate` and do not guess which one to use.
8. If exactly one matching agentbox container exists, fail clearly instead of reusing, replacing, or connecting to it. For a healthy running managed session, suggest `agentbox connect <directory>` or `agentbox stop <directory>`. For a transient `run`, suggest `agentbox stop <id>`.
9. If none exists, start detached `podman run --detach --rm` with the required mounts, default runtime image, runtime cache volume, canonical target working directory, and local-only published attach endpoint.
10. Pass `io.agentbox.container_kind=transient-run`, but do not pass the managed ownership label `io.agentbox.managed=true`.
11. Execute the selected runtime server command inside the container: `opencode serve --hostname 0.0.0.0 --port <port>` for OpenCode and `codex --dangerously-bypass-approvals-and-sandbox app-server --listen <endpoint> --ws-auth capability-token --ws-token-sha256 <token-sha256>` for Codex.
12. With the default `--dev-env auto`, start the runtime server command through the selected development environment wrapper, if one applies. With `--dev-env none`, start the runtime server command directly.
13. Wait for the published runtime server endpoint to become ready using the selected runtime's health check.
14. Execute the selected runtime host client from the canonical target directory with inherited stdin, stdout, and stderr: `opencode attach <endpoint>` for OpenCode and `codex --dangerously-bypass-approvals-and-sandbox --remote <endpoint> --remote-auth-token-env <env-var>` for Codex, followed by any `<agent-client-args>`.
15. After the host client exits, fails to start, or `run` is interrupted after the container starts, stop the transient container.
16. Exit with the host client process exit code when it is available.

Progress and diagnostics:

- `run` prints short `INFO` log progress to stderr while checking prerequisites, resolving session state, ensuring the runtime image, starting the transient server container, waiting up to 90 seconds for readiness, connecting, and stopping the transient container.
- Successful `run` does not write its own data to stdout. Runtime stdout and stderr come directly from the host client through inherited stdio.
- With `--verbose`, `run` also prints the external commands it executes.
- `run` does not print a connect suggestion and does not leave a managed session available for later `connect`.

Runtime rules:

- `run` accepts only `opencode` and `codex`.
- `--runtime` selects the runtime for the transient server container and host client when it is present.
- `--dev-env auto` is the default and enables automatic development environment loading for the runtime server command.
- `--dev-env none` disables automatic development environment loading for the runtime server command.
- `run` does not accept `--connect` or `-c`; clap rejects those options.
- Runtime client passthrough options such as `--no-alt-screen` are accepted only after the `--` delimiter. Without the delimiter, clap rejects them as `agentbox run` options.
- `run` does not pass `<agent-client-args>` to the transient runtime server command.
- Transient `run` is not a managed session. It is listed by `ps`, can be selected by `stop`, cannot be selected by `connect` or `health`, and does not create live managed metadata.
- If a managed session or transient `run` already exists for the resolved git root, `run` fails before reusing or comparing any stored runtime value.
- If the selected runtime host client command is missing, `run` fails before starting a container.
- If the selected runtime host client exits unsuccessfully, `run` stops the transient container and preserves the client exit code when available.
- If server readiness fails, `run` attempts to stop the transient container and reports cleanup failure alongside the readiness failure when both fail.
- `--runtime` does not change workspace identity.
- When `--runtime` is absent in an interactive terminal, the runtime prompt is rendered on stderr.
- Canceling the runtime prompt with Escape exits non-zero with `selection canceled`.
- Interrupting the runtime prompt with Ctrl-C exits non-zero with `selection interrupted`.
