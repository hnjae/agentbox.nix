# Agentbox Specification

This document specifies user-visible CLI behavior and operator-visible runtime
state for the `agentbox` CLI, installed package, managed Podman objects, and
documented filesystem effects. It does not specify Rust module structure or
private implementation details.

## Summary

`agentbox` is a Rust CLI for running code agents inside isolated Podman
containers.

The MVP is workspace-centric rather than name-centric:

- `agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] <directory>`
- `agentbox exec [--dev-env <auto|none>] <directory> [-- <codex-exec-args>...]`
- `agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] <directory>`
- `agentbox runtime update <opencode|codex>`
- `agentbox connect [directory]`
- `agentbox ls`
- `agentbox health`
- `agentbox stop [target]...`
- `agentbox clean`

`agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] <directory>`
resolves `<directory>` to its canonical git root, starts a transient runtime
server container, and connects to it with the selected runtime host-side
client from the canonical target directory. The transient container is not a
managed session and is not a target for `ls`, `connect`, `health`, or `stop`.
By default, `run` automatically starts the runtime server through the
applicable development environment for the launch directory; `--dev-env none`
disables that automatic wrapping. If `--runtime` is omitted in an interactive
terminal, `run` prompts for the runtime before validating runtime prerequisites
or starting a container.

`agentbox exec [--dev-env <auto|none>] <directory> [-- <codex-exec-args>...]`
resolves `<directory>` to its canonical git root and launches `codex exec` as a
Codex-only one-shot command in a foreground Podman container. The foreground
container is not a managed session and is not a target for `ls`, `connect`,
`health`, or `stop`. By default, `exec` automatically starts `codex exec`
through the applicable development environment for the launch directory;
`--dev-env none` disables that automatic wrapping. Any arguments intended for
`codex exec` must be passed after `--`.

`agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] <directory>`
resolves `<directory>` to its canonical git root and launches one managed
workspace session for that repository as a detached runtime server container.
The container starts the selected runtime server for that workspace. By
default, `start` automatically starts the server through the applicable
development environment for the launch directory; `--dev-env none` disables
that automatic wrapping. If `--connect` is set, `start` connects with the
runtime host-side client after the new runtime server endpoint is ready.

`agentbox connect [directory]` discovers the running server endpoint for the
resolved repository or selected session and runs the running session's runtime
host-side client command from the session's stored launch directory. A provided
directory is used only to identify the workspace once the session exists. For
`connect`, `<directory>` is a workspace selector, not a request to change the
running session's working directory. If no directory is provided in an
interactive terminal, `connect` prompts for one connectable running session.

Foreground `exec`, transient `run`, and managed containers are started with
Podman's `--rm` cleanup flag. When a foreground `exec` exits, when a transient
`run` container is stopped, when a managed runtime server exits, or when
`agentbox stop [target]...` stops a managed session, Podman removes the stopped
container. Default runtime images and the named runtime cache volume are
intentionally left for explicit later cleanup with `agentbox clean` or update.

If a matching managed session already exists for a repository, `run`, `exec`,
and `start` fail clearly instead of reusing, replacing, or changing it.

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

- the selected runtime host client command for `run`, and the running session's
  runtime host client command for `connect` and `start --connect`
- `npm` when `agentbox` must resolve the latest runtime npm package version
  for initial default image creation, a default runtime image rebuild after the
  embedded image build context changes, or `agentbox runtime update <runtime>`

For `run`, `exec`, `start`, and `connect`, `<directory>` must resolve to an
existing directory inside a git repository. A non-git target fails clearly; the
MVP does not create ad-hoc non-git sessions. `stop` normally follows the same
resolution rules, but it may also accept an exact stored git-root absolute path
string for a recoverable session whose stored path no longer exists.

Out of scope for the MVP:

- macOS or Windows support
- more than one valid managed session for the same canonical git root
- silent runtime switching or automatic recreation when an existing managed
  session has invalid runtime metadata
- user-supplied runtime image references
- generic container orchestration
- durable runtime state beyond the workspace bind mount, one Podman-managed
  runtime cache volume, Codex host configuration passthrough, OpenCode host
  state passthrough, runtime image version metadata, and live
  managed-container metadata
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

When `start` successfully launches a session, the canonical target directory
becomes the session's launch directory. The launch directory is recorded with the
session and remains the stable working-directory for later connects to that
running session. Transient `run` uses the canonical target directory as the
server container and host-client working directory, but it does not record
session metadata.

Required behavior:

- A symlinked path resolves to the same canonical git root as the real path.
- Nested repositories use the git root reported for the target directory, so an
  inner repository gets its own session.
- Submodules and git worktrees each get their own session identity because each
  resolves to its own canonical git root.
- Moving a repository to a different absolute path creates a new identity.
- A still-running container whose stored git-root path no longer exists is
  reported as `orphaned` until it is stopped.
- The target directory is not part of identity. A `start` invocation may choose
  any subdirectory under the git root as the launch directory for a new session,
  and a `connect` invocation may provide any subdirectory under the same git
  root to find that session. A transient `run` invocation may also choose any
  subdirectory under the git root as its one-off server and host-client working
  directory.
- `connect` target directories do not retarget a running session. They identify
  the workspace session, then the running session's stored launch directory
  controls host-client working directory.

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

Transient `run` containers are intentionally not managed containers. They use
the same deterministic runtime cache volume name for the workspace and publish
a local-only attach endpoint for the matching host client, but they do not
carry the managed-session marker label and they are not discovered by `ls`,
`connect`, `health`, or `stop`.

When a command is scoped to one canonical git root, containers that advertise a
different git-root identity token are outside that command's discovery scope and
must not block it. Containers with a missing identity token remain in scope
until full inspection proves whether they match.

When a deterministic container-name conflict is inspected, `agentbox` uses the
conflicting managed container's recoverable workspace identity labels to report
the conflicting git root before evaluating runtime-specific attach metadata.
Malformed runtime or attach metadata on a managed container for a different git
root must not hide that the failure is a different-workspace name conflict.

## CLI

Global flags:

- `--verbose` enables diagnostic command traces and external command output for
  commands that support verbose diagnostics. Diagnostic output is written to
  stderr and must not replace machine-readable or success output on stdout.

Global output rules:

- stdout is reserved for user-requested data output: `ls`, `health`, shell
  completion output, hidden completion root output, `--help`, and `--version`.
  Status messages, progress, success summaries, cancellation notices, verbose
  traces, forwarded external command output, and application errors are written
  to stderr.
- Application stderr logs use one line per message with this shape:
  `[2026-05-06T22:15:56+09:00] INFO: message`.
- Log timestamps use the local UTC offset. If the local offset cannot be
  determined, timestamps use UTC with `+00:00`.
- Log severities are `ERR`, `WARNING`, `INFO`, and `DEBUG`.
- ANSI color is used only when stderr is a TTY and `NO_COLOR` is not set.
  Timestamps and `DEBUG` labels are bright black, `ERR` labels are red,
  `WARNING` labels are yellow, and `INFO` labels are blue.
- Clap parse errors and usage text keep Clap's native stderr format. `--help`
  and `--version` keep Clap's native stdout format.
- Interactive prompt UI is rendered on stderr without being wrapped as log
  lines. `connect` runs the runtime host client with inherited stdio and does
  not wrap the client output as logs.

### `agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] <directory>`

`run` launches a transient selected-runtime server container, waits until its
local-only attach endpoint is ready, and connects with the selected runtime
host client.

Optional flags:

- `--runtime <opencode|codex>`
- `--dev-env <auto|none>`: choose whether `run` automatically starts the
  transient runtime server through the applicable development environment. The
  default is `auto`.

Expected behavior:

1. If `--runtime` is omitted and stdin and stderr are terminals, prompt on
   stderr with a fuzzy single-select list of supported runtimes. Use the
   selected runtime exactly as if the user had passed `--runtime`.
2. If `--runtime` is omitted and either stdin or stderr is not a terminal, fail
   before workspace or runtime validation with a clear error that `--runtime` is
   required in non-interactive use.
3. Validate Git availability and resolve `<directory>` to a canonical git root
   and canonical target directory.
4. Validate Podman, the selected runtime, the selected runtime host client
   command, and the host-attached Nix prerequisites.
5. Ensure concurrent lifecycle operations for the same git root do not leave
   duplicate sessions or ambiguous lifecycle state.
6. Discover existing managed containers for that canonical git root.
7. If more than one matching container exists, fail as `duplicate` and do not
   guess which one to use.
8. If exactly one matching managed container exists, fail clearly instead of
   reusing, replacing, or connecting to it. For a healthy running session,
   suggest `agentbox connect <directory>` or `agentbox stop <directory>`.
9. If none exists, start detached `podman run --detach --rm` with the required
   mounts, default runtime image, runtime cache volume, canonical target
   working directory, and local-only published attach endpoint.
10. Do not pass the managed-session marker label
    `io.agentbox.managed=true`.
11. Execute the selected runtime server command inside the container:
    `opencode serve --hostname 0.0.0.0 --port <port>` for OpenCode and
    `codex --dangerously-bypass-approvals-and-sandbox app-server --listen <endpoint>`
    for Codex.
12. With the default `--dev-env auto`, start the runtime server command through
    the selected development environment wrapper, if one applies. With
    `--dev-env none`, start the runtime server command directly.
13. Wait for the published runtime server endpoint to become ready using the
    selected runtime's health check.
14. Execute the selected runtime host client from the canonical target
    directory with inherited stdin, stdout, and stderr:
    `opencode attach <endpoint>` for OpenCode and
    `codex --dangerously-bypass-approvals-and-sandbox --remote <endpoint>` for
    Codex.
15. After the host client exits, fails to start, or `run` is interrupted after
    the container starts, stop the transient container.
16. Exit with the host client process exit code when it is available.

Progress and diagnostics:

- `run` prints short `INFO` log progress to stderr while checking
  prerequisites, resolving session state, ensuring the runtime image, starting
  the transient server container, waiting for readiness, connecting, and
  stopping the transient container.
- Successful `run` does not write its own data to stdout. Runtime stdout and
  stderr come directly from the host client through inherited stdio.
- With `--verbose`, `run` also prints the external commands it executes.
- `run` does not print a connect suggestion and does not leave a managed
  session available for later `connect`.

Runtime rules:

- `run` accepts only `opencode` and `codex` in the MVP.
- `--runtime` selects the runtime for the transient server container and host
  client when it is present.
- `--dev-env auto` is the default and enables automatic development
  environment loading for the runtime server command.
- `--dev-env none` disables automatic development environment loading for the
  runtime server command.
- `run` does not accept `--connect` or `-c`; clap rejects those options.
- Transient `run` is not a managed session. It is not listed by `ls`, cannot
  be selected by `connect`, `health`, or `stop`, and does not create live
  managed metadata.
- If a managed session already exists for the resolved git root, `run` fails
  before reusing or comparing any stored runtime value.
- If the selected runtime host client command is missing, `run` fails before
  starting a container.
- If the selected runtime host client exits unsuccessfully, `run` stops the
  transient container and preserves the client exit code when available.
- If server readiness fails, `run` attempts to stop the transient container and
  reports cleanup failure alongside the readiness failure when both fail.
- `--runtime` does not change workspace identity.
- When `--runtime` is absent in an interactive terminal, the runtime prompt is
  rendered on stderr.
- Canceling the runtime prompt with Escape exits non-zero with
  `selection canceled`.
- Interrupting the runtime prompt with Ctrl-C exits non-zero with
  `selection interrupted`.

### `agentbox exec [--dev-env <auto|none>] <directory> [-- <codex-exec-args>...]`

`exec` launches Codex exec in a foreground Podman container for one-shot,
non-server Codex tasks.

Optional flags and arguments:

- `--dev-env <auto|none>`: choose whether `exec` automatically starts
  `codex exec` through the applicable development environment. The default is
  `auto`.
- `<codex-exec-args>...`: arguments passed to `codex exec`. They must appear
  after a `--` delimiter so `agentbox` flags and Codex flags are unambiguous.

Expected behavior:

1. Validate Git availability and resolve `<directory>` to a canonical git root
   and canonical target directory.
2. Validate Podman, Codex, and the host-attached Nix prerequisites.
3. Ensure concurrent lifecycle operations for the same git root do not leave
   duplicate sessions or ambiguous lifecycle state.
4. Discover existing managed containers for that canonical git root.
5. If more than one matching container exists, fail as `duplicate` and do not
   guess which one to use.
6. If exactly one matching managed container exists, fail clearly instead of
   reusing, replacing, or connecting to it. For a healthy running session,
   suggest `agentbox connect <directory>` or `agentbox stop <directory>`.
7. If none exists, start foreground `podman run --rm --interactive` with the
   required Codex mounts, default Codex image, runtime cache volume, and
   canonical target working directory. If stdin, stdout, and stderr are
   terminals, also pass `--tty`.
8. Do not pass `--detach`, `--publish`, managed-session labels, or attach
   labels.
9. Execute Codex exec in YOLO mode in the container:
   `codex --dangerously-bypass-approvals-and-sandbox exec`, followed by any
   `<codex-exec-args>`.
10. With the default `--dev-env auto`, start the Codex exec command through the
    selected development environment wrapper, if one applies. With
    `--dev-env none`, start the Codex exec command directly.
11. Inherit stdin, stdout, and stderr for the Podman process.
12. Exit with the foreground Podman process exit code when it is available.

Progress and diagnostics:

- `exec` prints short `INFO` log progress to stderr while checking
  prerequisites, resolving session state, ensuring the Codex image, and
  starting the foreground container.
- Successful `exec` does not write its own data to stdout. Codex stdout and
  stderr come directly from the foreground container through inherited stdio.
- With `--verbose`, `exec` also prints the external commands it executes.
- `exec` does not wait for a runtime server endpoint, discover an attach
  endpoint, execute a host runtime client, or print a connect suggestion.

Runtime rules:

- `exec` is Codex-only in the MVP and does not accept `--runtime`.
- `--dev-env auto` is the default and enables automatic development
  environment loading for the Codex exec command.
- `--dev-env none` disables automatic development environment loading for the
  Codex exec command.
- `exec` does not accept `--connect` or `-c`; clap rejects those options.
- Foreground `exec` is not a managed session. It is not listed by `ls`, cannot
  be selected by `connect`, `health`, or `stop`, and does not create live
  managed metadata.
- If a managed session already exists for the resolved git root, `exec` fails
  before starting a foreground Codex container.
- `agentbox exec <directory>` with no Codex arguments is valid; Codex owns the
  resulting no-argument `codex exec` behavior.
- Codex passthrough options such as `--model` are accepted only after the `--`
  delimiter. Without the delimiter, clap rejects them as `agentbox exec`
  options.

### `agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] <directory>`

`start` launches a new workspace session as a detached runtime server.

Optional flags:

- `--connect`, `-c`: connect with the runtime host-side client after the new
  session is ready
- `--runtime <opencode|codex>`
- `--dev-env <auto|none>`: choose whether `start` automatically starts the
  runtime server through the applicable development environment. The default is
  `auto`.

Expected behavior:

1. If `--runtime` is omitted and stdin and stderr are terminals, prompt on
   stderr with a fuzzy single-select list of supported runtimes. Use the
   selected runtime exactly as if the user had passed `--runtime`.
2. If `--runtime` is omitted and either stdin or stderr is not a terminal, fail
   before workspace or runtime validation with a clear error that `--runtime` is
   required in non-interactive use.
3. Validate Git availability and resolve `<directory>` to a canonical git root
   and canonical target directory.
4. Validate Podman, the selected runtime, and the host-attached Nix
   prerequisites.
5. Ensure concurrent lifecycle operations for the same git root do not leave
   duplicate sessions or ambiguous lifecycle state.
6. Discover existing managed containers for that canonical git root.
7. If more than one matching container exists, fail as `duplicate` and do not
   guess which one to use.
8. If exactly one matching managed container exists, fail clearly instead of
   reusing or replacing it. For a healthy running session, suggest
   `agentbox connect <directory>` or `agentbox stop <directory>`.
9. If none exists, record the canonical target directory as the session launch
   directory and start detached `podman run --rm` with the required labels,
   mounts, default runtime image, local-only published attach endpoint, and
   launch-directory working directory.
10. Start the selected runtime server for the session. With the default
    `--dev-env auto`, the server starts through the selected development
    environment wrapper, if one applies. With `--dev-env none`, the server
    command starts directly.
11. Wait until the runtime server endpoint is ready for connection or the
    container exits.
12. If `--connect` is absent, report that the discovered attach endpoint is
    ready and suggest `agentbox connect <directory>`.
13. If `--connect` is present, report that the discovered attach endpoint is
    ready and execute the runtime host client command from the session's launch
    directory with stdio inherited, using the same client command behavior as
    `agentbox connect`.

Progress and diagnostics:

- `start` prints short `INFO` log progress to stderr while checking
  prerequisites, resolving session state, ensuring the runtime image, starting
  the detached container, and waiting for the runtime server endpoint.
- `start` prints its final success message as an `INFO` log on stderr.
  Successful `start` does not write to stdout.
- With `--verbose`, `start` also prints the external commands it executes and
  forwards non-JSON Podman command output as `DEBUG` logs on stderr.
- If the runtime container fails to start, exits before readiness, or times out
  before becoming reachable, `start` includes a short `podman logs --tail`
  excerpt for the managed container when Podman can provide one.

Runtime image wrapper output:

- During successful runtime image startup and later `/entrypoint` execution,
  agentbox-owned wrapper checks do not write internal probe values, such as
  resolved CA bundle paths, to stdout or stderr. Any stdout or stderr visible
  from `run`, `exec`, detached container logs, or later `/entrypoint` commands
  comes from the requested runtime command, selected development environment
  wrapper, or an explicit failure diagnostic.

Runtime rules:

- `start` accepts only `opencode` and `codex` in the MVP.
- `--runtime` selects the runtime for the new session when it is present.
- `--dev-env auto` is the default and enables automatic development
  environment loading for the server command.
- `--dev-env none` disables automatic development environment loading for the
  server command.
- `--connect` does not change session identity, runtime selection, container
  startup, endpoint readiness checks, or existing-session handling.
- If `--connect` is set and a managed session already exists for the resolved
  git root, `start` still fails before reusing or connecting to that session.
- When `--runtime` is absent in an interactive terminal, the runtime prompt is
  rendered on stderr and the final success message is an `INFO` log on stderr.
- Canceling the runtime prompt with Escape exits non-zero with
  `selection canceled`.
- Interrupting the runtime prompt with Ctrl-C exits non-zero with
  `selection interrupted`.
- After container launch has started, interrupting `start` with Ctrl-C before
  the final success message exits non-zero and triggers best-effort cleanup for
  resources created by that `start` invocation.
- Ctrl-C cleanup attempts to stop and remove the managed container created by
  the interrupted `start`.
- Ctrl-C cleanup removes the workspace cache volume only when that volume did
  not exist before the interrupted container launch.
- Ctrl-C cleanup does not remove the selected runtime's default image.
- If Ctrl-C cleanup cannot fully stop the container or remove an eligible cache
  volume, `start` reports a partial cleanup warning or error.
- If a managed session already exists for the resolved git root, `start` fails
  before reusing or comparing any stored runtime value.
- `--runtime` does not change session identity.
- If the host client launched by `start --connect` exits unsuccessfully or
  cannot be started, `start --connect` exits non-zero and reports that the
  managed session remains running so the user can retry
  `agentbox connect <directory>` or stop it with `agentbox stop <directory>`.

Image rules:

- `run` and `start` do not accept a user-supplied image reference.
- `run` and `start` always use the selected runtime's current default image
  reference.
- The default image may be built or reused by `agentbox`; users do not need to
  supply a build context.
- The selected runtime's current default image reference is
  `localhost/agentbox-<runtime>:ctx-<16-hex>`, where `<runtime>` is `opencode`
  or `codex` and `<16-hex>` is the first 16 lowercase hexadecimal characters
  of the SHA-256 digest over agentbox's embedded runtime image build context.
- The default image context hash is deterministic over these embedded image
  input paths and file contents: `Containerfile`, `bootstrap`, `entrypoint`,
  `lib/runtime-contract.sh`, and `runtime-packages.nix`. Documentation,
  developer tooling, and image tests such as `README.md`, `justfile`, and
  `tests/**` are not image inputs and do not affect the default image
  reference.
- If the selected runtime is `codex` and the current default image is missing,
  `run` or `start` resolves the latest `@openai/codex` npm version, builds the
  Codex default image with that version, and records the version metadata,
  image reference, and image context hash in agentbox state.
- If the selected runtime is `opencode` and the current default image is
  missing, `run` or `start` resolves the latest `opencode-ai` npm version,
  builds the OpenCode default image with that version, and records the version
  metadata, image reference, and image context hash in agentbox state.
- If the selected runtime's default image already exists, `run` or `start`
  reuses it without checking the npm registry.
- Existing legacy `localhost/agentbox-opencode:local` and
  `localhost/agentbox-codex:local` images are not current default image
  references and do not prevent `run` or `start` from building the current
  content-hash-tagged image.
- `agentbox` records the exact default image reference on a running managed
  `start` container so live discovery can report it while the container exists.
- Default runtime images are not removed by `stop`; image cleanup and image
  updates are explicit operator actions.

### `agentbox clean`

`clean` reclaims unused Podman resources owned by `agentbox`. It is a global
cleanup command, not a session stop command.

Optional flags:

- `--dry-run`: print cleanup candidates and skipped resources without deleting
  anything
- `--yes`: delete cleanup candidates without prompting
- `--images`: consider default runtime images only
- `--volumes`: consider workspace cache volumes only

Selection rules:

- If neither `--images` nor `--volumes` is set, `clean` considers both default
  runtime images and workspace cache volumes.
- If exactly one of `--images` or `--volumes` is set, only that resource kind
  is considered.
- `--dry-run` and `--yes` cannot be used together.

Image cleanup rules:

- `clean` considers agentbox-owned default runtime images discovered from
  image labels, including old content-hash-tagged default image references.
  Legacy pre-release exact image references such as
  `localhost/agentbox-opencode:local` and `localhost/agentbox-codex:local` are
  not current default image references and are not selected by `clean` unless
  they carry the current default-runtime-image labels.
- A default runtime image candidate is skipped when any Podman container,
  managed or unmanaged, currently uses that exact image reference.
- When the current state file for a runtime points to an image that was deleted
  successfully, the corresponding runtime image metadata file under
  `$XDG_STATE_HOME/agentbox/runtime` is removed. If state points to a different
  image that is still present or in use, it is preserved.
- `clean` does not remove image names by prefix and does not call
  `podman system prune` or Podman build-cache cleanup.

Volume cleanup rules:

- `clean` only considers named volumes whose names match the current workspace
  cache volume shape `agentbox-...-<12 hex>`.
- A candidate volume is skipped when any Podman container, managed or
  unmanaged, mounts that exact named volume source. Bind mounts whose host
  source path happens to match the volume name do not count as volume usage.
- `clean` does not call broad Podman volume pruning such as
  `podman volume prune --all`.

Confirmation and output rules:

- If no resources are cleanup candidates, `clean` emits an `INFO` log
  `nothing to clean` on stderr and exits successfully.
- With `--dry-run`, `clean` emits cleanup candidates and skip reasons as `INFO`
  logs on stderr, deletes nothing, and exits successfully.
- With `--yes`, `clean` deletes cleanup candidates without prompting.
- Without `--dry-run` or `--yes`, `clean` renders an interactive confirmation
  prompt on stderr only when stdin and stderr are terminals. The default answer
  is No. Case-insensitive `y` or `yes` approves cleanup; `n`, `no`, an empty
  response, or prompt cancellation emits a `WARNING` log `aborted` on stderr
  and exits successfully.
- Interrupting the confirmation prompt with Ctrl-C exits non-zero with
  `confirmation interrupted`.
- When stdin or stderr is not a TTY, `clean` fails unless `--yes` or
  `--dry-run` is set.
- If deletion of one candidate fails, `clean` continues deleting the remaining
  candidates, then exits non-zero with a summary of failed resources.
- Cleanup candidate, skip, deletion, abort, and no-op messages are stderr logs.
  Successful `clean` does not write to stdout.

Safety rules:

- `clean` never stops or removes running or stopped containers. Container
  lifecycle remains owned by `agentbox stop`.
- `clean` never deletes a workspace, the Nix store, `~/.codex`, or host
  OpenCode configuration or state directories.

### `agentbox runtime update <opencode|codex>`

`runtime update <opencode|codex>` refreshes the selected default runtime image.

Expected behavior:

1. Resolve the latest npm package version for the selected runtime:
   `opencode-ai` for OpenCode or `@openai/codex` for Codex.
2. Read the stored runtime image metadata from
   `$XDG_STATE_HOME/agentbox/runtime/opencode.json` or
   `$XDG_STATE_HOME/agentbox/runtime/codex.json`, if it exists.
3. Compute the selected runtime's current content-hash-tagged default image
   reference from the embedded image build context.
4. If the selected local default image exists and the stored installed version,
   image reference, and image context hash already match the latest npm version
   and current embedded context, skip the rebuild and record the latest check
   time.
5. Otherwise, rebuild the selected default image with the resolved npm version.
6. Record the installed version, latest seen version, check time, image build
   time, npm package name, install source, image reference, and image context
   hash in agentbox state.

Rules:

- The update command does not stop, replace, or mutate running sessions.
- Runtime image metadata files must include an image context hash; older
  pre-release state files missing that field are invalid runtime image
  metadata and are not migrated.
- The update command does not write metadata under runtime host state
  directories such as `~/.codex`, `${XDG_CONFIG_HOME:-$HOME/.config}/opencode`,
  or `${XDG_DATA_HOME:-$HOME/.local/share}/opencode`.
- Progress and result messages from `runtime update` are `INFO` logs on stderr.
  Successful `runtime update` does not write to stdout.

### `agentbox connect [directory]`

`connect` connects to an already-running managed workspace session. For
`connect`, a provided `<directory>` is a workspace selector, not a requested
working directory for the running session.

Expected behavior:

1. If `<directory>` is omitted and stdin and stderr are terminals, discover
   connectable running sessions and prompt on stderr with a fuzzy single-select
   list.
2. If `<directory>` is omitted and either stdin or stderr is not a terminal,
   fail with a clear error that a connect target is required in non-interactive
   use.
3. If `<directory>` is omitted and there are no connectable running sessions,
   fail without prompting and report that no connectable managed sessions exist.
4. If the selection prompt is canceled with Escape, exit non-zero with
   `selection canceled`.
5. If the selection prompt is interrupted with Ctrl-C, exit non-zero with
   `selection interrupted`.
6. Resolve the provided or selected directory to a canonical git root and
   canonical requested directory.
7. Discover the managed container for that canonical git root.
8. Fail if no matching managed session exists, and suggest
   `agentbox start --runtime <opencode|codex> <directory>`.
9. Fail as `duplicate` if more than one matching container exists.
10. Fail if the matching container is not running.
11. Discover the runtime attach endpoint and stored launch directory from
    managed-container metadata and Podman's published port data.
12. If the canonical requested directory differs from the stored launch
    directory, report that the requested directory was used only to identify the
    workspace and that `connect` is using the stored launch directory.
13. Execute the runtime host client command from the stored launch directory with
    stdio inherited, without re-evaluating or wrapping the client in any
    development environment.

Rules:

- `connect` never creates a new session.
- `connect` never starts or restarts a stopped session.
- `connect` never prompts for runtime selection.
- `connect` prompts for a target only when the positional directory is omitted.
- The connect prompt shows only connectable `running` sessions with recoverable
  git-root and endpoint metadata.
- The connect prompt is rendered on stderr and does not write to stdout before
  the runtime host client starts.
- `connect` does not accept or interpret `--runtime`.
- `connect` does not accept or interpret `--image`.
- `connect` does not use `podman attach` as the user transport in the MVP.
- `connect` does not open a raw shell through `podman exec`.
- The host client process current working directory is the running session's
  stored launch directory.
- The host client process uses the host environment of the `agentbox connect`
  invocation; `connect` does not re-evaluate `.envrc`, `devenv.nix`, or
  `flake.nix` from the requested directory or stored launch directory.
- For Codex sessions, the host client command includes
  `--dangerously-bypass-approvals-and-sandbox` when connecting with `--remote`,
  matching Codex 0.128.0 behavior that requires the YOLO flag on both the
  app-server and connecting client sides.
- When the requested directory differs from the stored launch directory,
  `connect` prints a short notice before launching the host client.
- The running server process keeps the working directory and environment from
  its original `start`.
- A different requested directory under the same git root does not change the
  running server or host client working directory for that `connect`.
- If the runtime client cannot be found on the host, `connect` fails clearly with
  the required command name.

### `agentbox ls`

`ls` lists managed workspace sessions from live Podman discovery.

Expected output fields:

- id, or `unknown`
- canonical git root, or `unknown`
- runtime, or `unknown`
- status
- endpoint, or `unknown`

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
- The public session id is the stable 12-character value from the
  `io.agentbox.git_root_hash` label. It is not the Podman container id.
- For `failed` sessions, fields that cannot be recovered from live Podman state
  are shown as `unknown`.
- By default, `ls` prints a compact borderless human-readable table.
- `ls --output table` and `ls -o table` explicitly select the same table output.
- `ls --output json`, `ls --output=json`, and `ls -o json` print a compact
  single-line JSON array followed by a newline.
- JSON rows contain stable keys: `id`, `canonical_git_root`, `runtime`,
  `status`, `endpoint`, and `container_name`.
- JSON keeps `container_name` for automation even though the table omits it.
- JSON uses `null` for unrecoverable `id`, `canonical_git_root`, `runtime`, or
  `endpoint` values instead of the table's `unknown` placeholder.
- JSON rows use the same ordering as table rows.
- Table output uses uppercase headers, no leading or trailing table padding, and
  ends with a newline.

### `agentbox health [target]`

`health` reports runtime health for currently running managed workspace
sessions from live Podman discovery. Without a target, it probes every running
session. With a target, it probes one running session selected by stable id
prefix.

Expected output fields:

- id
- canonical git root
- runtime
- health
- reason
- endpoint

Health values:

- `healthy`: the runtime's official health endpoint responded successfully.
- `unhealthy`: the runtime's official health endpoint did not respond with the
  runtime-specific healthy result, or required runtime/endpoint metadata was not
  recoverable from the running session.

Runtime probes:

- OpenCode is probed with `GET /global/health` on the discovered attach
  endpoint. The session is healthy only when the response is `HTTP 200` and the
  JSON response body has `healthy: true`.
- Codex is probed with `GET /readyz` on the discovered attach endpoint. The
  session is healthy only when the response is `HTTP 200`.

Rules:

- `health` includes only sessions whose discovered session status is `running`.
- Failed, stopped, orphaned, and duplicate sessions are not included.
- `health` probes each running session once and does not wait for recovery.
- `health <target>` treats `<target>` as a stable id prefix. Prefix matching is
  case-insensitive.
- If no session id matches the target prefix, `health <target>` fails clearly.
- If the prefix matches more than one distinct id, `health <target>` fails and
  asks for a longer prefix.
- If the selected session is not `running`, `health <target>` fails clearly
  instead of probing it.
- By default, `health` prints a compact borderless human-readable table.
- `health --output table` and `health -o table` explicitly select the same table
  output.
- `health --output json`, `health --output=json`, and `health -o json` print a
  compact single-line JSON array followed by a newline.
- JSON rows contain stable keys: `id`, `canonical_git_root`, `runtime`,
  `health`, `reason`, `endpoint`, and `container_name`.
- JSON keeps `container_name` for automation even though the table omits it.
- JSON uses `null` for unrecoverable `id`, `canonical_git_root`, `runtime`, or
  `endpoint` values instead of the table's `unknown` placeholder.
- JSON rows use the same ordering as table rows.
- Table output uses uppercase headers, no leading or trailing table padding, and
  ends with a newline.
- A healthy row uses reason `ok`.
- An unhealthy row uses a concise reason such as `unreachable`, `HTTP 503`,
  `malformed JSON`, or `healthy=false`.
- If there are no running sessions, `health` prints an empty table with headers
  by default, prints `[]` in JSON mode, and exits `0`.
- Unhealthy rows do not make the command fail; discovery or Podman failures
  remain command failures.

### `agentbox stop [target]...` / `agentbox stop --all`

`stop` stops workspace sessions for the resolved repositories, exact stored
git-root absolute paths, or stable id prefixes. It is not a volume pruning
command.
With `--all`, `stop` stops every running managed `agentbox` container.

Expected behavior:

1. If one or more `<target>` values are present, process each target and
   continue to later targets after a target-specific failure.
2. For each `<target>` that names an existing path, resolve it to a canonical
   git root.
3. For each `<target>` that is an absolute path that does not exist, require it
   to exactly match a live managed session's stored `io.agentbox.git_root`
   absolute path string. This selector may match orphaned sessions and failed
   sessions that still have a recoverable stored git-root path.
4. For each `<target>` that is not resolved as a path, treat it as a stable id
   prefix. Prefix matching is case-insensitive.
5. If no session id matches a target prefix, record a clear failure for that
   target.
6. If a prefix matches more than one distinct id, record a failure asking for a
   longer prefix.
7. If all prefix matches have the same full id, treat them as duplicate sessions
   for that id.
8. Ensure concurrent lifecycle operations for the same git root do not race.
9. Stop each matching container if it is running.
10. Treat an already-removed matching container as success after verifying it is
    absent.
11. If no matching managed session exists for a target, record that no session
    exists for the resolved repository or exact stored git-root path.
12. After all explicit targets are processed, exit non-zero if any target failed
    or any cleanup verification failed, and include a summary of the failed
    targets.
13. Rely on Podman's `--rm` cleanup for container removal after the stop.
14. Leave the runtime cache volume unmanaged by `stop` so it can be reclaimed
    later by explicit Podman volume cleanup.
15. If no `<target>` is present and `--all` is not set, discover stop
    candidates and prompt on stderr with a fuzzy multi-select list.
16. The no-target selector includes running, orphaned, duplicate, and failed
    sessions when a stable id is known, matching stop completion eligibility.
17. If the no-target selector is canceled with Escape, exit non-zero with
    `selection canceled`.
18. If the no-target selector is interrupted with Ctrl-C, exit non-zero with
    `selection interrupted`.
19. If no `<target>` is present and either stdin or stderr is not a terminal,
    fail with a clear error that a stop target or `--all` is required in
    non-interactive use.
20. If no selector candidates exist, print
    `agentbox stop: no managed sessions available to stop` as an `INFO` log on
    stderr and exit successfully without stopping anything.
21. If the selector returns an empty selection, print
    `agentbox stop: no sessions selected` as a `WARNING` log on stderr and exit
    successfully without stopping anything.
22. If `--all` is set, do not accept a `<target>` and stop all running managed
    sessions discovered from live Podman state.
23. `stop --all` stops running, orphaned, duplicate, and otherwise malformed
    managed containers whose Podman state is running.
24. `stop --all` ignores managed containers that are already stopped.
25. If `stop --all` finds no running managed containers, exit successfully.
26. For `stop --all`, lock each recoverable git root before stopping its
    currently running exact matches. Running managed containers without a
    recoverable git-root label are stopped only because the user selected the
    explicit global cleanup.

Optional flag:

- `--force`: best-effort cleanup when duplicate or failed exact matches exist
- `--all`: stop every running managed `agentbox` container

Safety rules:

- Without `--force`, `stop` fails when more than one matching managed container
  is found.
- With `--force`, `stop` stops all live managed containers that exactly claim
  the resolved canonical git root, exact stored git-root path, or selected
  stable id. It still does not stop containers that cannot be matched to that
  identity.
- With multiple explicit targets, `stop` may stop sessions for successful
  targets even when other targets fail.
- `--force` is not required with `--all`; `--all` already selects every running
  managed container.
- Stable id matching includes failed sessions, but `stop` only locks and stops
  a matched session when its git-root label is recoverable.
- `stop` never deletes the user workspace.
- `stop` never directly removes images or named cache volumes.
- Stop status and no-op messages are stderr logs. Successful `stop` does not
  write to stdout.

## Completion And Installed Assets

Shell completion for `connect`, `stop`, and `health` is dynamic.

Required behavior:

- Completion candidates come from live managed sessions, not from a static file.
- `connect` candidate values are canonical or stored git root paths when known.
  Sessions with no recoverable git-root path are not connect completion
  candidates, but remain visible through `agentbox ls`.
- `connect` completion includes only connectable `running` sessions with valid
  endpoint metadata.
- `stop` and `health` candidate values are stable ids.
- `stop` completion includes running, orphaned, duplicate, and failed sessions
  when a stable id is known.
- `stop` completion offers stable id candidates at every target position, not
  only the first target position.
- `health` completion includes sessions when a stable id is known.
- Candidate descriptions include root, runtime, and status when the shell
  supports descriptions.
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
  `share/man/man1/agentbox-start.1`, `share/man/man1/agentbox-connect.1`,
  `share/man/man1/agentbox-ls.1`, `share/man/man1/agentbox-health.1`,
  `share/man/man1/agentbox-stop.1`, `share/man/man1/agentbox-clean.1`,
  `share/man/man1/agentbox-runtime.1`, and
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

The runtime process runs as the image-local `user` account with UID `1000` and
home `/home/user`. The runtime user's primary GID inside the container is mapped
from the invoking host user's primary GID in Podman's user namespace. The
runtime also preserves the invoking host user's supplemental group access using
Podman's `keep-groups` behavior. A workspace file owned by, or group-writable
for, the invoking host user must therefore be accessible to the runtime user
according to normal host ownership and permission bits. `agentbox` must not
mutate workspace ownership or permissions to achieve this.

### Launch Directory CWD

The effective working directory for a running session is the stored launch
directory, not always the git root. `start` sets the launch directory from its
canonical target directory. `connect` uses the requested directory only to find
the workspace session, then runs the host client from the stored launch
directory. Transient `run` uses the canonical target directory as both the
runtime server container working directory and the host-client process working
directory, and does not create a stored launch directory.

Examples:

- command: `agentbox start --runtime opencode /aaa/bbb/subdir`
- mounted git root inside container: `/aaa/bbb`
- working directory seen by the runtime server: `/aaa/bbb/subdir`
- command: `agentbox connect /aaa/bbb/other`
- working directory of the host runtime client process: `/aaa/bbb/subdir`
- command: `agentbox run --runtime opencode /aaa/bbb/subdir`
- working directory seen by the runtime server and host client:
  `/aaa/bbb/subdir`

Rules:

- `start` starts the runtime server from the canonical target directory inside
  the container and records that directory as the session launch directory.
- `run` starts the transient runtime server from the canonical target directory
  inside the container, starts the runtime host client from the same canonical
  target directory on the host, and records no session metadata.
- `connect` starts the runtime host client from the stored launch directory on
  the host.
- `connect` does not change the already-running server process working
  directory.
- To use a different launch directory for the same git root, the user stops the
  current session and starts a new one from the desired directory.
- Runtime-specific remote project behavior must be provided by the runtime
  client/server protocol, not by `podman attach` or `podman exec`.

### Runtime Cache Volume

Each workspace identity has a writable runtime home at `/home/user`, backed by
the Podman-managed named runtime cache volume.

Rules:

- The runtime user home inside the container is `/home/user`.
- `/home/user` is mounted as the runtime cache volume and persists across later
  one-shot runs or detached sessions for the same canonical git root.
- Standard XDG parent directories under `/home/user`, including `.config`,
  `.cache`, `.local`, and `.local/state`, are writable by the runtime user.
- Runtime state written under `/home/user` survives container recreation unless
  a documented runtime passthrough mount or workspace bind mount shadows that
  subpath.
- The runtime cache volume name is identical to the deterministic managed
  container name for the same workspace identity.
- The mounted runtime cache volume stores Nix cache, evaluation artifacts, the
  active runtime profile, and other runtime home state that should survive later
  one-shot runs or detached sessions for the same canonical git root.
- A bind mount at `/home/user` does not satisfy the runtime cache
  volume requirement; the mount must be a Podman-managed named volume.
- The mounted runtime cache volume is owned or remapped so the runtime user can
  create home and cache files in it, including when a prior session created the
  volume under a different rootless Podman user namespace mapping.
- Existing named volumes are reused as-is. `agentbox` does not migrate or
  restructure volumes created by older releases that mounted only
  `/home/user/.cache/nix`.
- The runtime profile default path is `$XDG_STATE_HOME/nix/profile`.
- If `XDG_STATE_HOME` is unset and `HOME` is set, the runtime falls back to
  `$HOME/.local/state/nix/profile`.
- If both `XDG_STATE_HOME` and `HOME` are unavailable, the runtime falls back to
  `/home/user/.local/state/nix/profile`.
- No other subpath under `/home/user` is required to persist in the MVP unless
  it is named by a runtime-specific passthrough rule.
- `agentbox stop <directory>` does not explicitly delete the runtime cache
  volume.
- Once no container uses the cache volume, it remains available for explicit
  reclamation, for example with `podman volume rm <container-name>` or
  `podman volume prune --all`.
- Podman `--rm` removes the managed container, not the named runtime cache
  volume.

### SSH Commit Signing Passthrough

When the invoking host environment has a usable SSH agent socket, `run`,
`start`, and `exec` make SSH-based Git commit signing available inside the
runtime container without mounting private keys or host SSH configuration.

Rules:

- `agentbox` detects `SSH_AUTH_SOCK` on the host during launch preparation.
- If `SSH_AUTH_SOCK` is unset, container launch behavior is unchanged.
- If `SSH_AUTH_SOCK` is set but does not point to an accessible Unix socket,
  `agentbox` prints a warning, does not mount it, and continues launching the
  container.
- If `SSH_AUTH_SOCK` points to an accessible Unix socket, `agentbox` bind-mounts
  that socket at `/run/agentbox/ssh-agent.sock` and sets
  `SSH_AUTH_SOCK=/run/agentbox/ssh-agent.sock` inside the container.
- `agentbox` reads only the effective Git config values needed for commit
  signing from the host repository: `user.name`, `user.email`, `gpg.format`,
  `user.signingkey`, and `commit.gpgsign`.
- Those Git config values are injected into the container with Git's
  `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_*`, and `GIT_CONFIG_VALUE_*` environment
  variables.
- `agentbox` does not mount the host Git config files, credential helpers,
  `~/.ssh`, private keys, or GPG agent sockets for commit signing passthrough.
- If `user.signingkey` is an SSH public key literal, `agentbox` passes that
  literal unchanged.
- If `user.signingkey` is a public key file path, `agentbox` reads the public
  key file and passes the key literal instead of the path.
- If `user.signingkey` is a private key path, `agentbox` does not read the
  private key. If a sibling `<path>.pub` file exists and is readable,
  `agentbox` reads that public key and passes the key literal instead.
- `agentbox` does not verify that the configured signing key is currently
  loaded in the SSH agent. Git and ssh-agent own the final signing error if the
  agent cannot sign.
- GPG commit signing passthrough and Git SSH remote authentication are outside
  the MVP scope.

### Codex Host Configuration Passthrough

Codex sessions use the invoking host user's Codex configuration directory as
the runtime Codex home.

Rules:

- For `agentbox run --runtime codex` and `agentbox start --runtime codex`, the
  host `${HOME}/.codex` directory is bind-mounted read-write at
  `/home/user/.codex`.
- The mount is required so auth refreshes, skills, MCP configuration, plugins,
  rules, and other Codex user state remain consistent between host and
  container Codex clients.
- `run --runtime codex` and `start --runtime codex` fail before starting a
  container if `${HOME}/.codex` is missing, is not a directory, or is not
  readable and writable by the invoking host user.
- `agentbox` does not create, migrate, or write files inside `${HOME}/.codex`.
- OpenCode sessions do not receive the Codex passthrough mount.

### OpenCode Host State Passthrough

OpenCode sessions use the invoking host user's OpenCode configuration and data
directories as the runtime OpenCode state.

Rules:

- For `agentbox run --runtime opencode` and
  `agentbox start --runtime opencode`, the host
  `${XDG_CONFIG_HOME:-$HOME/.config}/opencode` directory is bind-mounted
  read-write at `/home/user/.config/opencode`.
- For `agentbox run --runtime opencode` and
  `agentbox start --runtime opencode`, the host
  `${XDG_DATA_HOME:-$HOME/.local/share}/opencode` directory is bind-mounted
  read-write at `/home/user/.local/share/opencode`.
- Both host directories are required so global configuration, provider
  settings, authentication state, and other OpenCode user state remain
  consistent between host and container OpenCode clients.
- `run --runtime opencode` and `start --runtime opencode` fail before starting
  a container if either host OpenCode directory is missing, is not a directory,
  or is not readable and writable by the invoking host user.
- `agentbox` validates the OpenCode state directories only. It does not require
  a specific authentication file such as `auth.json`, because OpenCode may also
  be configured through environment variables or provider configuration.
- `agentbox` does not create, migrate, or write files inside the host OpenCode
  configuration or data directories.
- Codex sessions do not receive the OpenCode passthrough mounts.

### Dev Environment Loading

When `run` or `start` uses the default `--dev-env auto`, it starts the runtime
command through the first applicable development environment provider for the
launch directory. For `run` and `start`, the runtime command is the runtime
server inside the container. The provider priority is:

1. `direnv`
2. `devenv`
3. `nix develop`
4. no wrapper

Rules:

- `run` and `start` use the canonical target directory as the runtime process
  working directory even when a development environment wrapper is selected.
  `start` also records that directory as the session launch directory.
- `run --dev-env none` and `start --dev-env none` disable automatic
  development environment loading and start the runtime command directly.
- Only the selected provider is used. If the selected provider command is
  missing, blocked, exits unsuccessfully, or otherwise fails during runtime
  process startup, the container startup fails through the normal error path and
  `agentbox` does not silently try a lower-priority provider.
- Development environment provider commands are executed inside the runtime
  container. `agentbox` does not require host-side `direnv` or `devenv`.
- `connect` starts the runtime host client directly from the stored launch
  directory and does not re-evaluate `.envrc`, `devenv.nix`, or `flake.nix`.
- When `start` launches a session, the server environment is fixed by the
  launch directory and development environment selection used for that `start`.
- `connect` to an already-running session does not reevaluate or replace the
  server environment.
- The MVP does not persist development environment selection or state for
  running-session compatibility checks.
- The MVP does not compare a different requested connect directory against the
  earlier `start` development environment context for that running session.

`direnv` selection:

- A matching `.envrc` applies when `.envrc` exists in the canonical target
  directory or in any ancestor up to and including the canonical git root.
- If a matching `.envrc` applies, `run --dev-env auto` or
  `start --dev-env auto` starts the runtime command as
  `direnv exec . <runtime argv>` from the canonical target directory.

`devenv` selection:

- `devenv` is considered only when no matching `.envrc` applies.
- The selected `devenv.nix` is the closest `devenv.nix` found in the canonical
  target directory or in any ancestor up to and including the canonical git
  root.
- If a `devenv.nix` is selected, `run --dev-env auto` or
  `start --dev-env auto` starts the runtime command as
  `devenv shell --no-tui --from path:<root> -- <runtime argv>`, where `<root>`
  is the directory containing the selected `devenv.nix`.

`nix develop` selection:

- `nix develop` is considered only when no matching `.envrc` or `devenv.nix`
  applies.
- The selected flake is the closest `flake.nix` found in the canonical target
  directory or in any ancestor up to and including the canonical git root.
- Automatic flake selection considers only `devShells.<system>.<attr>`.
  `packages` and `legacyPackages` are not automatic development environment
  candidates.
- If the selected `flake.nix` is in the canonical target directory, `run
  --dev-env auto` or `start --dev-env auto` looks for the `default` dev shell.
- If the selected `flake.nix` is in a parent directory of the canonical target
  directory, `run --dev-env auto` or `start --dev-env auto` first looks for a
  dev shell named `basename(<directory>)`, then falls back to `default`.
- If a candidate dev shell exists, `run --dev-env auto` or
  `start --dev-env auto` starts the runtime command as
  `nix develop --no-write-lock-file path:<flake_root>#<attr> --command
  <runtime argv>`.
- If the selected flake can be evaluated but none of the candidate dev shells
  exists, `run --dev-env auto` or `start --dev-env auto` starts the runtime
  command directly.
- If automatic flake evaluation itself fails for reasons other than a missing
  candidate dev shell attribute, `run` or `start` fails clearly before starting a
  container.

### Runtime Server And Client Behavior

OpenCode managed sessions:

- use OpenCode's remote server and host-side connection client
- expose an `http` attach endpoint
- run with `OPENCODE_CONFIG_CONTENT={"autoupdate":false}` so OpenCode
  auto-update behavior does not change the installed runtime version inside the
  managed image or mutate host configuration
- run with `OPENCODE_PERMISSION='{"*":"allow"}'` so OpenCode receives an allow
  permission map for every permission key

Codex managed sessions:

- use Codex's app server and host-side remote client
- expose a `ws` attach endpoint

Endpoint rules:

- The server listens inside the container on the runtime's configured listen
  address and container port.
- The runtime server command must pass the configured listen address to
  runtimes whose default bind address would not be reachable through the
  published attach endpoint.
- For OpenCode `http` attach endpoints, `run` and `start` treat the endpoint as
  ready only after `GET /global/health` on the same host-published endpoint
  returns `HTTP 200` and a JSON response body whose `healthy` field is `true`.
  A TCP connection, TCP accept followed by a reset, arbitrary HTTP response,
  malformed JSON response, or health response with `healthy: false` is not
  sufficient readiness.
- For Codex `ws` attach endpoints, `run` and `start` treat the endpoint as
  ready only after `GET /readyz` on the same host-published endpoint returns
  `HTTP 200`. A TCP connection alone is not sufficient readiness.
- The attach endpoint is published only on the host loopback interface by
  default.
- The default host attach IP is `127.0.0.1`.
- `agentbox` may let Podman allocate the host port, but it must discover the
  concrete host port from Podman before reporting success from `start` or
  executing the `run`, `start --connect`, or `connect` host client.
- The attach endpoint must be discoverable from the runtime's attach
  specification plus Podman's published port data. For managed sessions, stored
  managed-container metadata must also be consistent with that endpoint.
- The host client command is executed with inherited stdio. `run` executes it
  from the canonical target directory, while `start --connect` and `connect`
  execute it from the running session's stored launch directory.

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
  `/etc/static/nix`, `run` or `start` treats `/etc/static/nix` as needed even
  when that static file ultimately resolves into `/nix/store`.
- Runtime profile state lives under `$XDG_STATE_HOME/nix/profile`, with fallback
  to `$HOME/.local/state/nix/profile` or `/home/user/.local/state/nix/profile`
  when needed.
- The Codex default image installs Codex from npm package `@openai/codex` at
  the version resolved by `agentbox` for that image build.
- The OpenCode default image installs OpenCode from npm package `opencode-ai`
  at the version resolved by `agentbox` for that image build.
- The runtime image provides its own CA bundle. Host SSL trust-store mounts are
  out of scope for the MVP.
- If a host-attached Nix prerequisite is missing, `run` or `start` fails
  clearly and does not attempt to synthesize a bundled Nix installation.
- If the selected runtime host client command is missing, `run` fails clearly
  before starting a transient container.

## Lifecycle And Drift Recovery

Valid lifecycle behavior:

- `run` creates a transient runtime server container, connects with the
  selected host client, stops the transient container before exiting, and does
  not create a managed session.
- `start` creates the workspace session as a detached runtime server container.
- `connect` discovers an existing running workspace session and runs the runtime
  host client against its published endpoint.
- `ls` derives session status from live Podman state and host path checks.
- `stop` stops the container and relies on the container's `--rm` run option
  for container cleanup.
- Concurrent lifecycle operations for the same canonical git root do not leave
  more than one valid managed session or ambiguous cleanup outcome.

Default runtime image lifecycle is separate from managed sessions. When a
foreground `exec` exits, when a transient `run` container is stopped, when a
managed runtime server exits, or when `agentbox stop` stops a managed session,
Podman removes the container but keeps the default runtime image. Current
default runtime images are tagged by embedded build-context content hash.
Runtime image package updates happen through
`agentbox runtime update <opencode|codex>`, and unused current, old
content-hash-tagged, or legacy `:local` default images can be removed through
`agentbox clean`; `stop` does not remove or rebuild images.

Named runtime cache volume lifecycle remains separate. Transient `run`,
foreground `exec`, and `agentbox stop` leave the workspace cache volume intact
so later one-shot runs or detached sessions can reuse it. Volume reclamation is
explicit through `agentbox clean` or direct Podman commands.

Required drift behavior:

- Duplicate containers for one git root: mark the session as `duplicate`, fail
  `run`, `start`, and `connect`, and do not guess which container to use.
  `stop --force` may stop all duplicate managed containers that exactly claim
  the resolved canonical git root or exact stored git-root path.
- Missing or malformed managed-container metadata: mark the session as `failed`
  and require explicit cleanup or recreation before the session can be used
  again.
- Missing runtime cache volume mount for an existing session, including a bind
  mount where the named volume is expected: fail clearly and require explicit
  container recreation.
- Missing or inconsistent attach endpoint metadata or published port data: mark
  the session as `failed` and require explicit cleanup or recreation before the
  session can be connected.
- Missing host-attached Nix prerequisite: fail clearly, report the missing
  mount, client, socket, config, or state-path requirement, and do not attempt
  to synthesize a bundled Nix installation.
- Runtime image setup failure: fail clearly and preserve inspectable runtime
  state when Podman has not already removed the container.
- Identity collision between different canonical git roots: fail clearly and do
  not treat them as the same workspace.
- Stop failure: report exactly which managed containers are still running or
  still inspectable.
- A `failed` session is not connectable. If enough metadata remains to identify
  it by git root or exact stored git-root path,
  `agentbox stop --force <directory>` may stop it. If the session cannot be
  matched safely, `ls` reports the concrete container name and the user must
  remove that container with Podman before starting a new session for the
  affected workspace.

## Error Handling

The CLI must produce actionable errors that say what failed, which workspace was
involved, which external command failed when relevant, and what the user can try
next.

Required error cases:

- non-git target directory
- requested directory escapes the resolved git root
- unsupported runtime
- unsupported `run --connect` or `run -c`
- Podman not installed
- Git not installed
- unsupported or malformed runtime metadata on an existing managed session
- container failed to start
- runtime server command not found
- runtime host client command not found
- connect failed
- missing or inconsistent attach endpoint metadata
- missing published attach port
- duplicate managed containers for one git root
- `run` or `start` called for a git root that already has a managed session
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
- missing or unusable host `${HOME}/.codex` for `run --runtime codex` or
  `start --runtime codex`
- missing or unusable host OpenCode configuration or data directories for
  `run --runtime opencode` or `start --runtime opencode`
- missing host `npm` when a runtime npm version must be resolved
- unusable runtime profile path under the XDG state or HOME fallback location
- runtime image setup failure
- selected development environment wrapper unavailable, blocked, or failing
  during runtime process startup
- automatic flake evaluation failure while resolving a `nix develop` wrapper
- workspace or host Nix permission problems that prevent required access
- `clean` run from non-TTY stdin without `--yes` or `--dry-run`
- partial `clean` deletion failures

## Security And Isolation

MVP isolation expectations:

- separate rootless Podman container per transient run, foreground exec, or
  workspace session
- explicit workspace mount only for the canonical git root
- host-provided Nix inputs mounted alongside one Podman-managed cache volume
- Codex sessions receive the invoking host user's `${HOME}/.codex` directory as
  a read-write passthrough mount
- OpenCode sessions receive the invoking host user's OpenCode configuration and
  data directories as read-write passthrough mounts
- one writable Podman-managed named cache volume at `/home/user`
- minimal privileges
- networking enabled only as needed for the runtime command and, for detached
  sessions, the runtime server's local-only published attach endpoint
- attach endpoints bound to host loopback by default, not all host interfaces

Runtime user and bind-mount rules:

- The container runs as the non-root image-local `user` account with UID `1000`
  and home `/home/user`.
- The runtime user's primary GID is the invoking host user's primary GID as
  mapped by Podman's user namespace configuration.
- The invoking host user's supplemental groups are preserved for bind-mount
  permission checks using Podman's `keep-groups` behavior.
- The workspace bind mount is read-write by default.
- The Podman-managed runtime cache volume at `/home/user` is writable by the
  runtime user.
- Host ownership and permission bits remain authoritative.
- `agentbox` must not `chown`, `chmod`, remount, or elevate privileges to force
  access.
- If the runtime user cannot read or write a required path inside the bind
  mount, `run`, `start`, or `connect` fails clearly with the affected path and
  the permission problem.
- `agentbox` must not repair host workspace permissions by mutating the host
  mount.
- `agentbox` must not repair host Nix access by mutating host permissions or
  host configuration.

Out of scope for MVP:

- hardened sandbox guarantees beyond normal rootless Podman isolation
- secret brokering or policy-based filesystem mediation
- cross-host or multi-user orchestration
