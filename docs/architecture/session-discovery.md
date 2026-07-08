# Session Discovery Boundary

The session discovery layer owns live Podman inspection, agentbox ownership classification, managed versus transient classification, status derivation, endpoint recovery, and detection of duplicate, orphaned, failed, and name-conflict states.

## Contracts

- Live Podman container state is the source of truth for running managed sessions and transient `run` containers.
- Agentbox must not require a host-side session database to discover, connect to, stop, restart, list, or health-check running sessions.
- Managed-session container labels own recoverable session metadata: managed ownership marker, schema version, canonical git root, stable identity token, runtime, default image reference, launch directory, logical name, attach metadata, stored server arguments, and stored resource limits.
- Transient `run` container labels own only the metadata needed for discovery, `ps`, `stop`, endpoint recovery during the owning process, and collision detection; transient containers must not carry the managed ownership marker.
- Discovery scoped to one workspace must use the stable identity token when available and must treat missing tokens conservatively until full inspection proves whether a container matches.
- Duplicate detection must include both managed sessions and transient `run` containers that claim the same stable workspace identity; command-specific candidate filters must not hide duplicate runtime-server state.
- State classification is derived from live discovery at command time.
- Failed, orphaned, duplicate, stopped, and transient resources must not be silently coerced into valid managed running sessions.
- Managed endpoint metadata in labels and Podman's published-port data must agree before a session is connectable.
- Session lifecycle commands must never infer container ownership from image names, image labels, or container-name patterns alone; container ownership requires an agentbox ownership label.
- Name-conflict reporting must use recoverable workspace identity labels before runtime-specific metadata so malformed runtime or endpoint labels cannot hide a different-workspace name conflict.
- Discovery must normalize missing or `null` JSON collections to empty collections so unrelated ambient containers cannot cause lifecycle failures.
- Commands must fail closed on malformed managed-session metadata that affects identity, runtime selection, endpoint recovery, launch directory, server arguments, resource limits, cache volume mounts, or authorization.
- Duplicate resources must be surfaced as duplicates and never resolved by arbitrary ordering.
