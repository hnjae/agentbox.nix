# Runtime Catalog Boundary

The runtime catalog owns runtime-specific server commands, host-client commands, readiness probes, health probes, endpoint schemes, default image identities, host state passthrough requirements, and Codex attach-token requirements.

## Contracts

- Runtime-specific command construction must flow through the runtime catalog so supported runtimes share lifecycle orchestration while preserving distinct server, client, readiness, health, image, and passthrough contracts.
- Readiness and health checks must use the runtime catalog's official runtime probe, not raw TCP success.
