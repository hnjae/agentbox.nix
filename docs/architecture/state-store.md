# State Store Boundary

The state store owns durable persistence mechanics only for agentbox state that is not recoverable from live Podman state: runtime image metadata and Codex managed-session attach tokens. Domain services own the meaning of those records and must not use the state store as a session database.

## Contracts

- State records must never make a session discoverable without matching live Podman container state.
- State writes must expose either a complete durable record or a recoverable missing/invalid state to later readers.
- Codex managed-session attach tokens are stored in agentbox state and are addressed by the session identity; only token hashes are passed into managed containers.
- Codex attach-token generation, hashing, state storage, environment injection, and missing-token errors must be owned by a single token service so server and client sides cannot diverge.
- Stale Codex token state must not authorize a nonmatching session identity or runtime.
- Capability tokens must not be written to container labels, command-line arguments visible inside the managed container, or user-facing output.
