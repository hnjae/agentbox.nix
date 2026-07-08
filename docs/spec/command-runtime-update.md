# `agentbox runtime update <opencode|codex|--all|-a>`

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
