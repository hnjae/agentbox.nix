# Container Execution Boundary

The container execution adapter owns Podman command construction, inspect result normalization, container stop verification, published-port discovery, mount construction, resource-limit application, and mapping of Podman failures into domain errors.

## Contracts

- Container assembly must bind-mount the canonical git root at the same absolute path and set the runtime process working directory from the launch contract selected by the command.
- The container adapter must mount `/home/user` as the workspace runtime cache named volume and must reject bind mounts or missing mounts where a named volume is required.
- The container adapter must receive resolved runtime, mounts, environment, labels, command argv, working directory, stdio mode, publication mode, and resource limits from lifecycle/domain services; it must not read user config, discover sessions, select runtimes, or inspect durable state to fill missing launch contracts.
- Managed container creation must apply the resolved resource limits to Podman and include the same resolved values in managed-session metadata. Transient `run` container creation applies resolved limits without creating managed-session metadata.
- Attach endpoints must be published on loopback by default and discovered from Podman published-port data before reporting readiness or launching a host client.
- External process failures must be mapped without losing the underlying command name and exit status when available.
