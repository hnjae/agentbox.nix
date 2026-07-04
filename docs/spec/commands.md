# Commands

Global flags:

- `--verbose` enables diagnostic command traces and external command output for commands that support verbose diagnostics. Diagnostic output is written to stderr and must not replace machine-readable or success output on stdout.

Global output rules:

- stdout is reserved for user-requested data output: `ps`, `health`, shell completion output, hidden completion root output, `--help`, and `--version`. Status messages, progress, success summaries, cancellation notices, verbose traces, forwarded external command output, and application errors are written to stderr.
- Application stderr logs use one line per message with this shape: `[2026-05-06T22:15:56+09:00] INFO: message`.
- Log timestamps use the local UTC offset. If the local offset cannot be determined, timestamps use UTC with `+00:00`.
- Log severities are `ERR`, `WARNING`, `INFO`, and `DEBUG`.
- ANSI color is used only when stderr is a TTY and `NO_COLOR` is not set. Timestamps and `DEBUG` labels are bright black, `ERR` labels are red, `WARNING` labels are yellow, and `INFO` labels are blue. In the `selected development environment` info log, the selected provider name or `none` is bold bright cyan.
- Clap parse errors and usage text keep Clap's native stderr format. `--help` and `--version` keep Clap's native stdout format.
- Interactive prompt UI is rendered on stderr without being wrapped as log lines. `connect` runs the runtime host client with inherited stdio and does not wrap the client output as logs.

## `agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-client-args>...]`

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

## `agentbox exec [--dev-env <auto|none>] [directory] [-- <codex-exec-args>...]`

`exec` launches Codex exec in a foreground Podman container for one-shot, non-server Codex tasks.

Optional flags and arguments:

- `--dev-env <auto|none>`: controls automatic development environment wrapping for `codex exec`. The default is `auto`.
- `<codex-exec-args>...`: arguments passed to `codex exec`. They must appear after a `--` delimiter so `agentbox` flags and Codex exec flags are unambiguous.

Expected behavior:

1. Validate Git availability and resolve `[directory]` to a canonical git root and canonical target directory. If `[directory]` is omitted, resolve `.`.
2. Validate Podman, Codex, and the host-attached Nix prerequisites.
3. Ensure concurrent lifecycle operations for the same git root do not leave duplicate sessions or ambiguous lifecycle state.
4. Discover existing managed containers for that canonical git root.
5. If more than one matching container exists, fail as `duplicate` and do not guess which one to use.
6. If exactly one matching managed container exists, fail clearly instead of reusing, replacing, or connecting to it. For a healthy running session, suggest `agentbox connect <directory>` or `agentbox stop <directory>`.
7. If none exists, start foreground `podman run --rm --interactive` with the required Codex mounts, default Codex image, runtime cache volume, and canonical target working directory. If stdin, stdout, and stderr are terminals, also pass `--tty`.
8. Do not pass `--detach`, `--publish`, managed-session labels, or attach labels.
9. Execute Codex exec in YOLO mode in the container: `codex --dangerously-bypass-approvals-and-sandbox exec --disable codex_git_commit`, followed by any `<codex-exec-args>`.
10. With the default `--dev-env auto`, start the Codex exec command through the selected development environment wrapper, if one applies. With `--dev-env none`, start the Codex exec command directly.
11. Inherit stdin, stdout, and stderr for the Podman process.
12. Exit with the foreground Podman process exit code when it is available.

Progress and diagnostics:

- `exec` prints short `INFO` log progress to stderr while checking prerequisites, resolving session state, ensuring the Codex image, and starting the foreground container.
- Successful `exec` does not write its own data to stdout. Codex stdout and stderr come directly from the foreground container through inherited stdio.
- With `--verbose`, `exec` also prints the external commands it executes.
- `exec` does not wait for a runtime server endpoint, discover an attach endpoint, execute a host runtime client, or print a connect suggestion.

Runtime rules:

- `exec` is Codex-only and does not accept `--runtime`.
- `--dev-env auto` is the default and enables automatic development environment loading for the Codex exec command.
- `--dev-env none` disables automatic development environment loading for the Codex exec command.
- `exec` does not accept `--connect` or `-c`; clap rejects those options.
- Foreground `exec` is not a managed session. It is not listed by `ps`, cannot be selected by `connect`, `health`, or `stop`, and does not create live managed metadata.
- If a managed session already exists for the resolved git root, `exec` fails before starting a foreground Codex container.
- `agentbox exec [directory]` with no Codex arguments is valid; Codex owns the resulting no-argument `codex exec` behavior.
- Codex passthrough options such as `--model` are accepted only after the `--` delimiter. Without the delimiter, clap rejects them as `agentbox exec` options.

## `agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-server-args>...]`

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

Runtime image wrapper output:

- During successful runtime image startup and later `/entrypoint` execution, agentbox-owned wrapper checks do not write internal probe values, such as resolved CA bundle paths, to stdout or stderr. Any stdout or stderr visible from `run`, `exec`, detached container logs, or later `/entrypoint` commands comes from the requested runtime command, selected development environment wrapper, or an explicit failure diagnostic.

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

## `agentbox restart [--connect|-c] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [target]`

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
- Stopped, failed, orphaned, and duplicate managed sessions are not restartable. They require explicit `agentbox stop` cleanup followed by `agentbox start` if a new session is desired.
- Restart is not all-or-nothing. After the old container is stopped, a later replacement start or readiness failure can leave no running managed session.

Image rules:

- `run`, `start`, and `restart` do not accept a user-supplied image reference.
- `run`, `start`, and `restart` always use the selected runtime's current default image reference.
- The default image may be built or reused by `agentbox`; users do not need to supply a build context.
- The selected runtime's current default image reference is `localhost/agentbox-<runtime>:ctx-<16-hex>`, where `<runtime>` is `opencode` or `codex` and `<16-hex>` is the first 16 lowercase hexadecimal characters of the SHA-256 digest over agentbox's embedded runtime image build context.
- The default image context hash is deterministic over agentbox's embedded runtime image build inputs and their contents. Documentation, developer tooling, and image tests are not image inputs and do not affect the default image reference.
- If the selected runtime is `codex` and the current default image is missing, `run`, `start`, or `restart` resolves the latest `@openai/codex` npm version, builds the Codex default image with that version, and records the version metadata, image reference, and image context hash in agentbox state.
- If the selected runtime is `opencode` and the current default image is missing, `run`, `start`, or `restart` resolves the latest `opencode-ai` npm version, builds the OpenCode default image with that version, and records the version metadata, image reference, and image context hash in agentbox state.
- If the selected runtime's default image already exists, `run`, `start`, or `restart` reuses it without checking the npm registry.
- Image references that do not match the selected runtime's current default image reference do not satisfy the default image contract and do not prevent `run`, `start`, or `restart` from building the current content-hash-tagged image.
- `agentbox` records the exact default image reference on a running managed `start` or `restart` container so live discovery can report it while the container exists.
- Default runtime images are not removed by `stop`; image cleanup and image updates are explicit operator actions.

## `agentbox clean`

`clean` reclaims unused Podman resources and lock files owned by `agentbox`. It is a global cleanup command, not a session stop command.

Optional flags:

- `--dry-run`: print cleanup candidates and skipped resources without deleting anything
- `--yes`: delete cleanup candidates without prompting
- `--images`: consider default runtime images only
- `--volumes`: consider workspace cache volumes only
- `--locks`: consider workspace lock files only

Selection rules:

- If none of `--images`, `--volumes`, or `--locks` is set, `clean` considers default runtime images, workspace cache volumes, and workspace lock files.
- If any of `--images`, `--volumes`, or `--locks` is set, only the selected resource kinds are considered.
- `--dry-run` and `--yes` cannot be used together.

Image cleanup rules:

- `clean` considers agentbox-owned default runtime images discovered from image labels, including old content-hash-tagged default image references. Image names or references alone do not make an image eligible for cleanup unless the image also carries agentbox default-runtime-image ownership labels.
- A default runtime image candidate is skipped when any Podman container, managed or unmanaged, currently uses that exact image reference.
- When the current state file for a runtime points to an image that was deleted successfully, the corresponding runtime image metadata file under `$XDG_STATE_HOME/agentbox/runtime` is removed. If state points to a different image that is still present or in use, it is preserved.
- `clean` does not remove image names by prefix and does not call `podman system prune` or Podman build-cache cleanup.

Volume cleanup rules:

- `clean` only considers named volumes whose names match the current workspace cache volume shape `agentbox-...-<12 hex>`.
- A candidate volume is skipped when any Podman container, managed or unmanaged, mounts that exact named volume source. Bind mounts whose host source path happens to match the volume name do not count as volume usage.
- `clean` does not call broad Podman volume pruning such as `podman volume prune --all`.

Lock file cleanup rules:

- `clean` only considers lock files whose paths match the current workspace lock shape `$XDG_STATE_HOME/agentbox/locks/<64 lowercase hex>.lock`.
- A candidate lock file is skipped when `agentbox` cannot acquire its workspace lock without blocking, which indicates that another process currently holds the lock.
- Files in the lock directory that do not match the workspace lock file name shape, symlinks, directories, and lock-like files outside the current workspace lock directory are ignored.

Confirmation and output rules:

- If no resources are cleanup candidates and no resources are skipped, `clean` emits an `INFO` log `nothing to clean` on stderr and exits successfully.
- If resources are skipped but no resources are cleanup candidates, `clean` emits the skip reasons on stderr and exits successfully without prompting.
- With `--dry-run`, `clean` emits cleanup candidates and skip reasons as `INFO` logs on stderr, deletes nothing, and exits successfully.
- With `--yes`, `clean` deletes cleanup candidates without prompting.
- Without `--dry-run` or `--yes`, `clean` renders an interactive confirmation prompt on stderr only when stdin and stderr are terminals. The default answer is No. Case-insensitive `y` or `yes` approves cleanup; `n`, `no`, an empty response, or prompt cancellation emits a `WARNING` log `aborted` on stderr and exits successfully.
- Interrupting the confirmation prompt with Ctrl-C exits non-zero with `confirmation interrupted`.
- When stdin or stderr is not a TTY, `clean` fails unless `--yes` or `--dry-run` is set.
- If deletion of one candidate fails, `clean` continues deleting the remaining candidates, then exits non-zero with a summary of failed resources.
- Cleanup candidate, skip, deletion, abort, and no-op messages are stderr logs. Successful `clean` does not write to stdout.

Safety rules:

- `clean` never stops or removes running or stopped containers. Container lifecycle remains owned by `agentbox stop`.
- `clean` never deletes a workspace, the Nix store, `~/.codex`, or host OpenCode configuration or state directories.

## `agentbox runtime update <opencode|codex|--all|-a>`

`runtime update <opencode|codex>` refreshes the selected default runtime image. `runtime update --all` and `runtime update -a` refresh every supported default runtime image.

Arguments:

- `<opencode|codex>` selects one default runtime image to refresh.
- `--all` or `-a` selects every supported default runtime image. It is mutually exclusive with a runtime argument.

Expected behavior:

1. Resolve the latest npm package version for the selected runtime: `opencode-ai` for OpenCode or `@openai/codex` for Codex.
2. Read the stored runtime image metadata from `$XDG_STATE_HOME/agentbox/runtime/opencode.json` or `$XDG_STATE_HOME/agentbox/runtime/codex.json`, if it exists.
3. Compute the selected runtime's current content-hash-tagged default image reference from the embedded image build context.
4. If the selected local default image exists and the stored installed version, image reference, and image context hash already match the latest npm version and current embedded context, skip the rebuild and record the latest check time.
5. Otherwise, rebuild the selected default image with the resolved npm version.
6. Record the installed version, latest seen version, check time, image build time, npm package name, install source, image reference, and image context hash in agentbox state.

For `--all` or `-a`, `runtime update` applies the same behavior to supported runtimes sequentially in supported-runtime order: `opencode`, then `codex`. If one runtime update fails, later runtimes are not attempted.

Rules:

- The update command does not stop, replace, or mutate running sessions.
- Runtime image metadata files must include an image context hash; files missing required fields are invalid runtime image metadata and are replaced only by successful image setup or runtime update.
- The update command does not write metadata under runtime host state directories such as `~/.codex`, `${XDG_CONFIG_HOME:-$HOME/.config}/opencode`, or `${XDG_DATA_HOME:-$HOME/.local/share}/opencode`.
- Progress and result messages from `runtime update` are `INFO` logs on stderr. Successful `runtime update` does not write to stdout.

## `agentbox connect [directory] [-- <agent-client-args>...]`

`connect` connects to an already-running managed workspace session. For `connect`, a provided `<directory>` is a workspace selector, not a requested working directory for the running session.

Optional arguments:

- `<agent-client-args>...`: arguments passed to the selected runtime host client. They must appear after a `--` delimiter so `agentbox` flags and agent flags are unambiguous.

Expected behavior:

1. If `<directory>` is omitted and stdin and stderr are terminals, discover connectable running sessions and prompt on stderr with a fuzzy single-select list.
2. If `<directory>` is omitted and either stdin or stderr is not a terminal, fail with a clear error that a connect target is required in non-interactive use.
3. If `<directory>` is omitted and there are no connectable running sessions, fail without prompting and report that no connectable managed sessions exist.
4. If the selection prompt is canceled with Escape, exit non-zero with `selection canceled`.
5. If the selection prompt is interrupted with Ctrl-C, exit non-zero with `selection interrupted`.
6. Resolve the provided or selected directory to a canonical git root and canonical requested directory.
7. Discover the managed container for that canonical git root.
8. Fail if no matching managed session exists, and suggest `agentbox start --runtime <opencode|codex> <directory>`.
9. Fail as `duplicate` if more than one matching container exists.
10. Fail if the matching container is not running.
11. Discover the runtime attach endpoint and stored launch directory from managed-container metadata and Podman's published port data.
12. If the canonical requested directory differs from the stored launch directory, report that the requested directory was used only to identify the workspace and that `connect` is using the stored launch directory.
13. Execute the runtime host client command from the stored launch directory with stdio inherited, without re-evaluating or wrapping the client in any development environment, followed by any `<agent-client-args>`.

Rules:

- `connect` never creates a new session.
- `connect` never starts or restarts a stopped session.
- `connect` never prompts for runtime selection.
- `connect` prompts for a target only when the positional directory is omitted.
- The connect prompt shows only connectable `running` managed sessions with recoverable git-root and endpoint metadata.
- Transient `run` containers are never connect candidates.
- The connect prompt is rendered on stderr and does not write to stdout before the runtime host client starts.
- `connect` does not accept or interpret `--runtime`.
- `connect` does not accept or interpret `--image`.
- Runtime client passthrough options such as `--no-alt-screen` are accepted only after the `--` delimiter. Without the delimiter, clap rejects them as `agentbox connect` options.
- `connect` may prompt for a target while still accepting `<agent-client-args>` after `--`.
- `connect` does not use `podman attach` as the user transport.
- `connect` does not open a raw shell through `podman exec`.
- The host client process current working directory is the running session's stored launch directory.
- The host client process uses the host environment of the `agentbox connect` invocation; `connect` does not re-evaluate `.envrc`, `devenv.nix`, or `flake.nix` from the requested directory or stored launch directory.
- For Codex sessions, the host client command includes `--dangerously-bypass-approvals-and-sandbox` when connecting with `--remote`, matching Codex behavior that requires the YOLO flag on both the app-server and connecting client sides.
- For Codex sessions, the host client command also passes `--remote-auth-token-env AGENTBOX_CODEX_REMOTE_TOKEN`, and `agentbox` sets that environment variable to the capability token for the selected session.
- When the requested directory differs from the stored launch directory, `connect` prints a short notice before launching the host client.
- The running server process keeps the working directory and environment from its original `start`.
- A different requested directory under the same git root does not change the running server or host client working directory for that `connect`.
- If the runtime client cannot be found on the host, `connect` fails clearly with the required command name.

## `agentbox ps`

`ps` lists agentbox-owned managed workspace sessions and transient `run` containers from live Podman discovery.

Expected output fields:

- id, or `unknown`
- type
- canonical git root, or `unknown`
- runtime, or `unknown`
- status
- endpoint, or `unknown`

Type values:

- `managed`: a managed workspace session.
- `run`: a transient `run` container.

Status values:

- `running`: the agentbox container exists and is running.
- `orphaned`: the agentbox container exists and is running, but the stored git root path no longer exists on the host.
- `duplicate`: more than one agentbox container claims the same canonical git root.
- `failed`: the agentbox container exists, but required metadata, workspace mounts, published endpoint data, or other inspectable session invariants are inconsistent.

Rules:

- Containers not marked as managed sessions or transient `run` containers by `agentbox` are ignored, even if their names resemble `agentbox` names.
- The public session id is the stable 12-character value from the `io.agentbox.git_root_hash` label. It is not the Podman container id.
- For `failed` sessions, fields that cannot be recovered from live Podman state are shown as `unknown`.
- By default, `ps` prints a compact borderless human-readable table.
- `ps --output table` and `ps -o table` explicitly select the same table output.
- `ps --output json`, `ps --output=json`, and `ps -o json` print a compact single-line JSON array followed by a newline.
- JSON rows contain stable keys: `id`, `type`, `canonical_git_root`, `runtime`, `status`, `endpoint`, and `container_name`.
- JSON keeps `container_name` for automation even though the table omits it.
- JSON uses `null` for unrecoverable `id`, `canonical_git_root`, `runtime`, or `endpoint` values instead of the table's `unknown` placeholder.
- JSON rows use the same ordering as table rows.
- Table output uses uppercase headers, no leading or trailing table padding, and ends with a newline.

## `agentbox health [target]`

`health` reports runtime health for currently running managed workspace sessions from live Podman discovery. Without a target, it probes every running session. With a target, it probes one running session selected by stable id prefix or workspace directory.

Expected output fields:

- id
- canonical git root
- runtime
- health
- reason
- endpoint

Health values:

- `healthy`: the runtime's official health endpoint responded successfully.
- `unhealthy`: the runtime's official health endpoint did not respond with the runtime-specific healthy result, or required runtime/endpoint metadata was not recoverable from the running session.

Runtime probes:

- OpenCode is probed with `GET /global/health` on the discovered attach endpoint. The session is healthy only when the response is `HTTP 200` and the JSON response body has `healthy: true`.
- Codex is probed with `GET /readyz` on the discovered attach endpoint. The session is healthy only when the response is `HTTP 200`.

Rules:

- `health` includes only sessions whose discovered session status is `running`.
- Failed, stopped, orphaned, and duplicate sessions are not included.
- Transient `run` containers are not included or probed.
- `health` probes each running session once and does not wait for recovery.
- `health <target>` treats an existing path as a workspace directory and resolves it to a canonical git root.
- `health <target>` treats a missing relative path or other non-path target as a stable id prefix. Prefix matching is case-insensitive.
- If no running managed session matches the target, `health <target>` fails clearly.
- If the target matches more than one distinct id or workspace session, `health <target>` fails and asks for a more specific target.
- If the selected session is not `running`, `health <target>` fails clearly instead of probing it.
- By default, `health` prints a compact borderless human-readable table.
- `health --output table` and `health -o table` explicitly select the same table output.
- `health --output json`, `health --output=json`, and `health -o json` print a compact single-line JSON array followed by a newline.
- JSON rows contain stable keys: `id`, `canonical_git_root`, `runtime`, `health`, `reason`, `endpoint`, and `container_name`.
- JSON keeps `container_name` for automation even though the table omits it.
- JSON uses `null` for unrecoverable `id`, `canonical_git_root`, `runtime`, or `endpoint` values instead of the table's `unknown` placeholder.
- JSON rows use the same ordering as table rows.
- Table output uses uppercase headers, no leading or trailing table padding, and ends with a newline.
- A healthy row uses reason `ok`.
- An unhealthy row uses a concise reason such as `unreachable`, `HTTP 503`, `malformed JSON`, or `healthy=false`.
- If there are no running sessions, `health` prints an empty table with headers by default, prints `[]` in JSON mode, and exits `0`.
- Unhealthy rows do not make the command fail; discovery or Podman failures remain command failures.

## `agentbox stop [target]...` / `agentbox stop --all`

`stop` stops managed workspace sessions or transient `run` containers for the resolved repositories, exact stored git-root absolute paths, or stable id prefixes. It is not a volume pruning command. With `--all`, `stop` stops every running managed or transient `run` agentbox-owned container.

Expected behavior:

1. If one or more `<target>` values are present, process each target and continue to later targets after a target-specific failure.
2. For each `<target>` that names an existing path, resolve it to a canonical git root.
3. For each `<target>` that is an absolute path that does not exist, require it to exactly match a live managed session or transient `run` container's stored `io.agentbox.git_root` absolute path string. This selector may match orphaned sessions and failed resources that still have a recoverable stored git-root path.
4. For each `<target>` that is not resolved as a path, treat it as a stable id prefix. Prefix matching is case-insensitive.
5. If no session id matches a target prefix, record a clear failure for that target.
6. If a prefix matches more than one distinct id, record a failure asking for a longer prefix.
7. If all prefix matches have the same full id, treat them as duplicate sessions for that id.
8. Ensure concurrent lifecycle operations for the same git root do not race.
9. Stop each matching container if it is running.
10. Treat an already-removed matching container as success after verifying it is absent.
11. If no matching agentbox container exists for a target, record that no resource exists for the resolved repository or exact stored git-root path.
12. After all explicit targets are processed, exit non-zero if any target failed or any cleanup verification failed, and include a summary of the failed targets.
13. Rely on Podman's `--rm` cleanup for container removal after the stop.
14. Leave the runtime cache volume unmanaged by `stop` so it can be reclaimed later by explicit Podman volume cleanup.
15. If no `<target>` is present and `--all` is not set, discover stop candidates and prompt on stderr with a fuzzy multi-select list.
16. The no-target selector includes running, orphaned, duplicate, and failed managed sessions and transient `run` containers when a stable id is known, matching stop completion eligibility.
17. If the no-target selector is canceled with Escape, exit non-zero with `selection canceled`.
18. If the no-target selector is interrupted with Ctrl-C, exit non-zero with `selection interrupted`.
19. If no `<target>` is present and either stdin or stderr is not a terminal, fail with a clear error that a stop target or `--all` is required in non-interactive use.
20. If no selector candidates exist, print `agentbox stop: no agentbox containers available to stop` as an `INFO` log on stderr and exit successfully without stopping anything.
21. If the selector returns an empty selection, print `agentbox stop: no sessions selected` as a `WARNING` log on stderr and exit successfully without stopping anything.
22. If `--all` is set, do not accept a `<target>` and stop all running managed sessions and transient `run` containers discovered from live Podman state.
23. `stop --all` stops running, orphaned, duplicate, and otherwise malformed managed or transient `run` containers whose Podman state is running.
24. `stop --all` ignores agentbox containers that are already stopped.
25. If `stop --all` finds no running agentbox containers, exit successfully.
26. For `stop --all`, lock each recoverable git root before stopping its currently running exact matches. Running agentbox containers without a recoverable git-root label are stopped only because the user selected the explicit global cleanup.

Optional flag:

- `--force`: best-effort cleanup when duplicate or failed exact matches exist
- `--all`: stop every running managed or transient `run` agentbox container

Safety rules:

- Without `--force`, `stop` fails when more than one matching agentbox container is found.
- With `--force`, `stop` stops all live agentbox containers that exactly claim the resolved canonical git root, exact stored git-root path, or selected stable id. It still does not stop containers that cannot be matched to that identity.
- With multiple explicit targets, `stop` may stop sessions for successful targets even when other targets fail.
- `--force` is not required with `--all`; `--all` already selects every running managed or transient `run` agentbox container.
- Stable id matching includes failed sessions. When a matched session has a recoverable git-root label, `stop` uses that git root for locking; when only a stable id is recoverable, `stop` may stop only exact live matches for that id and must not expand the selection to unrelated containers.
- `stop` never deletes the user workspace.
- `stop` never directly removes images or named cache volumes.
- Stop status and no-op messages are stderr logs. Successful `stop` does not write to stdout.
