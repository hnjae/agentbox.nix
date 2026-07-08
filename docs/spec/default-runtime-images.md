# Default Runtime Images

Runtime image wrapper output:

- During successful runtime image startup and later `/entrypoint` execution, agentbox-owned wrapper checks do not write internal probe values, such as resolved CA bundle paths, to stdout or stderr. Any stdout or stderr visible from `run`, `exec`, detached container logs, or later `/entrypoint` commands comes from the requested runtime command, selected development environment wrapper, or an explicit failure diagnostic.

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
