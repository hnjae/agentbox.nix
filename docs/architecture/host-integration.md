# Host Integration Boundary

The host integration layer owns host Git identity lookup, Git excludes lookup, SSH signing passthrough, SSH known-host discovery, user config loading, resource-limit default resolution, host-attached Nix validation, runtime host-client lookup, and development-environment wrapper selection.

## Contracts

- User config is a strict input contract. Invalid config is isolated by moving it aside when possible and is ignored for the current invocation.
- User config loading must produce a single validated config result per command invocation so known-host passthrough and resource-limit defaults cannot interpret the same file differently.
- Resource-limit default resolution is owned by host integration. For `run` and `start`, each CLI limit overrides the corresponding config default and otherwise resolves to unlimited; for `restart`, each CLI limit overrides the stored managed-session value and config defaults are not re-applied.
- Host state passthroughs for Codex, OpenCode, Git identity, Git excludes, SSH signing, known_hosts, and host-attached Nix must be assembled during launch preparation and passed into container construction as explicit mount and environment contracts.
- Host passthrough lookups must use an explicit launch repository: the resolved canonical git root for new container launches, and the recovered managed-session git root for `restart`.
- Temporary host files used as mount sources must live at least until the corresponding container mount is established and must not become durable agentbox state.
- Agentbox must not repair host permissions, mutate workspace ownership, create runtime host state directories, or synthesize missing host-attached Nix prerequisites.
- Development-environment wrapper selection must be determined from the launch directory before runtime command execution; after a provider is selected, provider startup failure is terminal and must not fall back to a lower-priority provider.
- Development-environment selection is an invocation-time launch decision and must not be persisted as managed-session compatibility metadata.
- Host client processes must inherit stdio and run from the command-specific working directory defined by the spec.
- Loopback client connections must preserve inherited proxy variables while ensuring loopback hosts are excluded through both `NO_PROXY` and `no_proxy`.
