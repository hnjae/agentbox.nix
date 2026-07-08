# Runtime Connectivity

## Runtime Server And Client Behavior

OpenCode server containers, including transient `run` and managed sessions:

- use OpenCode's remote server and host-side connection client
- expose an `http` attach endpoint
- run with `OPENCODE_CONFIG_CONTENT={"autoupdate":false}` so OpenCode auto-update behavior does not change the installed runtime version inside the managed image or mutate host configuration
- run with `OPENCODE_PERMISSION='{"*":"allow"}'` so OpenCode receives an allow permission map for every permission key

Codex server containers, including transient `run` and managed sessions:

- use Codex's app server and host-side remote client
- expose a `ws` attach endpoint
- use Codex WebSocket capability-token authentication on the app server and host-side remote client

Endpoint rules:

- The server listens inside the container on the runtime's configured listen address and container port.
- The runtime server command must pass the configured listen address to runtimes whose default bind address would not be reachable through the published attach endpoint.
- For OpenCode `http` attach endpoints, `run`, `start`, and `restart` treat the endpoint as ready only after `GET /global/health` on the same host-published endpoint returns `HTTP 200` and a JSON response body whose `healthy` field is `true`. A TCP connection, TCP accept followed by a reset, arbitrary HTTP response, malformed JSON response, or health response with `healthy: false` is not sufficient readiness.
- For Codex `ws` attach endpoints, `run`, `start`, and `restart` treat the endpoint as ready only after `GET /readyz` on the same host-published endpoint returns `HTTP 200`. A TCP connection alone is not sufficient readiness.
- The attach endpoint is published only on the host loopback interface by default.
- The default host attach IP is `127.0.0.1`.
- `agentbox` may let Podman allocate the host port, but it must discover the concrete host port from Podman before reporting success from `start` or `restart`, or executing the `run`, `start --connect`, `restart --connect`, or `connect` host client.
- The attach endpoint must be discoverable from the runtime's attach specification plus Podman's published port data. For managed sessions, stored managed-container metadata must also be consistent with that endpoint.
- The host client command is executed with inherited stdio. `run` executes it from the canonical target directory, while `start --connect`, `restart --connect`, and `connect` execute it from the running session's stored launch directory.
- When launching a host client for a loopback attach endpoint, `agentbox` preserves inherited proxy variables and ensures loopback hosts bypass proxies by augmenting both `NO_PROXY` and `no_proxy`.
- Codex app-server commands that listen on the container-wide attach address must use capability-token authentication. `agentbox` passes only the token SHA-256 to the container server command.
- For transient Codex `run`, the attach token is held only for the lifetime of the `agentbox run` process.
- For managed Codex `start` and `restart`, the attach token is stored under `$XDG_STATE_HOME/agentbox/codex/ws-tokens/` and read by later `agentbox connect`. Missing token state makes `connect` fail clearly and require session restart or recreation.
