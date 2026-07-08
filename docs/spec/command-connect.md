# `agentbox connect [directory] [-- <agent-client-args>...]`

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
7. Discover agentbox-owned containers for that canonical git root and derive managed-session eligibility.
8. Fail as `duplicate` if discovery finds duplicate runtime-server state for the workspace, including a managed session plus a transient `run` container claiming the same identity.
9. Fail if no matching managed session exists, and suggest `agentbox start --runtime <opencode|codex> <directory>`.
10. Fail if the matching managed session is not running.
11. Discover the runtime attach endpoint and stored launch directory from managed-container metadata and Podman's published port data.
12. If the canonical requested directory differs from the stored launch directory, report that the requested directory was used only to identify the workspace and that `connect` is using the stored launch directory.
13. Execute the runtime host client command from the stored launch directory with stdio inherited, without re-evaluating or wrapping the client in any development environment, followed by any `<agent-client-args>`.

Rules:

- `connect` never creates a new session.
- `connect` never starts or restarts a stopped session.
- `connect` never prompts for runtime selection.
- `connect` prompts for a target only when the positional directory is omitted.
- The connect prompt shows only connectable `running` managed sessions with recoverable git-root and endpoint metadata.
- Transient `run` containers are never connect candidates. A transient `run` container that claims the same workspace as a managed session makes the workspace duplicate and prevents `connect` until cleanup.
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
