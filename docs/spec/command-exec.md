# `agentbox exec [--dev-env <auto|none>] [directory] [-- <codex-exec-args>...]`

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
