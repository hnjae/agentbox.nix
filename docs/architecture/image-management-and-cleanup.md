# Image Management And Cleanup Boundary

The image manager owns content-hash-tagged default image references, runtime package version resolution, default image build or reuse decisions, image ownership labels, runtime image metadata semantics, and image update semantics. The state store owns durable persistence mechanics for runtime image metadata.

## Contracts

- Runtime image metadata under agentbox state records image version and build facts, but it must not be required to recognize or stop running sessions.
- Named runtime cache volumes are owned by Podman and keyed by workspace identity. Session lifecycle commands must preserve them unless the explicit cleanup command selects them.
- The image manager must compute the selected runtime's current default image reference from the embedded runtime image inputs only. Runtime package versions affect image build contents and metadata, but must not affect the context-hash tag unless they are represented in the embedded image inputs.
- The image manager must own the stable embedded runtime image input set used for the context hash and must exclude documentation, developer tooling, and image tests from that hash.
- Runtime package version resolution must be separate from default image reference derivation so rebuild and update decisions can distinguish package-version changes from embedded-context changes.
- Image build inputs, image labels, runtime package metadata, and runtime image metadata must be consistent enough for `run`, `start`, `restart`, `runtime update`, and `clean` to make the same ownership and reuse decisions.
- Default image creation and `runtime update` may mutate runtime image metadata; session lifecycle commands may read it but must not depend on it to discover live sessions.
- `clean` must discover image cleanup candidates from agentbox image ownership labels, volume cleanup candidates from workspace cache volume naming rules, and lock file cleanup candidates from workspace lock path rules.
- `clean` must preserve each runtime's current default image by comparing its validated runtime and content-hash-tagged reference with the default reference computed from the executing binary's embedded image inputs. This protection must not depend on the agentbox package version, runtime package version, runtime image state, or network version resolution.
- Runtime package freshness belongs exclusively to `runtime update`; cleanup must not turn package refresh into an implicit consequence of reclaiming unused resources.
- Workspace lock cleanup must coordinate with workspace lock acquisition so cleanup does not unlink a lock path that a lifecycle command can continue using as an unlinked lock file.
- Cleanup must skip resources used by any Podman container, must continue after per-resource deletion failures, and must never call broad Podman prune operations.
- `stop` and `restart` must never delete default runtime images or named runtime cache volumes.
