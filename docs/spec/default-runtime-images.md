# Default Runtime Images

Runtime image wrapper output:

- During successful runtime image startup and later `/entrypoint` execution, agentbox-owned wrapper checks do not write internal probe values, such as resolved CA bundle paths, to stdout or stderr. Any stdout or stderr visible from `run`, `exec`, detached container logs, or later `/entrypoint` commands comes from the requested runtime command, selected development environment wrapper, or an explicit failure diagnostic.

Image rules:

- `run`, `start`, `restart`, and `exec` do not accept a user-supplied image reference.
- `run`, `start`, `restart`, and `exec` always use the selected runtime's current default image reference.
- The default image may be built or reused by `agentbox`; users do not need to supply a build context.
- The selected runtime's current default image reference is `localhost/agentbox-<runtime>:ctx-<16-hex>`, where `<runtime>` is `opencode` or `codex` and `<16-hex>` is the first 16 lowercase hexadecimal characters of the SHA-256 digest over agentbox's embedded runtime image build context.
- The default image context hash is deterministic over agentbox's embedded runtime image build inputs and their contents. Documentation, developer tooling, and image tests are not image inputs and do not affect the default image reference.
- If the selected runtime is `codex` and the current default image is missing, `run`, `start`, `restart`, or `exec` resolves the latest `@openai/codex` npm version, builds the Codex default image with that version, and records the version metadata, image reference, and image context hash in agentbox state.
- If the selected runtime is `opencode` and the current default image is missing, `run`, `start`, or `restart` resolves the latest `opencode-ai` npm version, builds the OpenCode default image with that version, and records the version metadata, image reference, and image context hash in agentbox state.
- If the selected runtime's default image already exists, `run`, `start`, `restart`, or `exec` reuses it without checking the npm registry.
- Image references that do not match the selected runtime's current default image reference do not satisfy the default image contract and do not prevent `run`, `start`, `restart`, or `exec` from building the current content-hash-tagged image.
- `agentbox` records the exact default image reference on a running managed `start` or `restart` container so live discovery can report it while the container exists.
- Default runtime images are not removed by `stop`; image cleanup and image updates are explicit operator actions.

Image creation feedback:

- When stderr is an interactive terminal and `TERM` is not `dumb`, default image creation in `run`, `start`, `restart`, or `exec` shows automatically refreshed progress for long-running version resolution and image build stages. Progress identifies the image, runtime package version when known, current stage, and elapsed time without claiming a percentage completion.
- `NO_COLOR` disables progress color without disabling interactive progress.
- When stderr is not an interactive terminal or `TERM=dumb`, default image creation writes stable line-oriented `INFO` start and result logs to stderr without ANSI sequences or carriage returns.
- With `--verbose`, default image creation uses static stage logs instead of interactive progress and forwards both Podman stdout and stderr as line-oriented `DEBUG` logs to stderr while the build is running. Forwarded output remains captured for build-failure diagnostics.
- Image progress and forwarded build output never write to stdout. Interactive progress is cleared before success or failure diagnostics are written.
