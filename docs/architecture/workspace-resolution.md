# Workspace Resolution Boundary

The workspace resolver owns directory validation, git-root discovery, canonical path resolution, target-directory containment checks, and construction of the stable workspace identity token and public id.

## Contracts

- Workspace identity is derived only from the canonical git root after symlink resolution and target-directory containment validation.
- Runtime, launch directory, requested command path spelling, and ambient Podman state must not alter workspace identity.
- The public id must be a stable 12-lowercase-hex value derived from the canonical git root and must be the same value used in identity labels, deterministic names, completion candidates, and machine-readable output.
- Deterministic managed container names and runtime cache volume names must be derived from the same workspace identity.
- The naming algorithm must be stable for a canonical git root and must fail on name conflicts or identity collisions rather than generating alternate names.
- Stable public ids are domain identifiers derived from the workspace identity label; Podman container ids must not be exposed as session ids.
