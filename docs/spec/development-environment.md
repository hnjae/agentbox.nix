# Development Environment

When `run`, `start`, or `restart` uses the default `--dev-env auto`, it starts the runtime command through the first applicable development environment provider for the launch directory. For `run`, `start`, and `restart`, the runtime command is the runtime server inside the container. The provider priority is:

1. `direnv`
2. `devenv`
3. `nix develop`
4. no wrapper

Rules:

- `run` and `start` use the canonical target directory as the runtime process working directory even when a development environment wrapper is selected. `start` also records that directory as the session launch directory.
- `restart` uses the stored launch directory as the runtime process working directory and re-evaluates wrapper selection from that directory.
- `run --dev-env none`, `start --dev-env none`, and `restart --dev-env none` disable automatic development environment loading and start the runtime command directly.
- Only the selected provider is used. If the selected provider command is missing, blocked, exits unsuccessfully, or otherwise fails during runtime process startup, the container startup fails through the normal error path and `agentbox` does not silently try a lower-priority provider.
- Development environment provider commands are executed inside the runtime container. `agentbox` does not require host-side `direnv` or `devenv`.
- `connect` starts the runtime host client directly from the stored launch directory and does not re-evaluate `.envrc`, `devenv.nix`, or `flake.nix`.
- When `start` launches a session, the server environment is fixed by the launch directory and development environment selection used for that `start`.
- When `restart` replaces a session, the server environment is recomputed from the stored launch directory and selected `--dev-env` mode.
- `connect` to an already-running session does not reevaluate or replace the server environment.
- `agentbox` does not persist development environment selection or state for running-session compatibility checks.
- `agentbox` does not compare a different requested connect directory against the earlier `start` development environment context for that running session.

`direnv` selection:

- A matching `.envrc` applies when `.envrc` exists in the launch directory or in any ancestor up to and including the canonical git root.
- If a matching `.envrc` applies, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command as `direnv exec . <runtime argv>` from the launch directory.

`devenv` selection:

- `devenv` is considered only when no matching `.envrc` applies.
- The selected `devenv.nix` is the closest `devenv.nix` found in the launch directory or in any ancestor up to and including the canonical git root.
- If a `devenv.nix` is selected, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command as `devenv shell --no-tui -- <runtime argv>` from the launch directory, allowing `devenv` to resolve the nearest project configuration using its normal working-directory behavior.

`nix develop` selection:

- `nix develop` is considered only when no matching `.envrc` or `devenv.nix` applies.
- The selected flake is the closest `flake.nix` found in the launch directory or in any ancestor up to and including the canonical git root.
- Automatic flake selection considers only `devShells.<system>.<attr>`. `packages` and `legacyPackages` are not automatic development environment candidates.
- If the selected `flake.nix` is in the launch directory, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` looks for the `default` dev shell.
- If the selected `flake.nix` is in a parent directory of the launch directory, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` first looks for a dev shell named `basename(<directory>)`, then falls back to `default`.
- If a candidate dev shell exists, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command as `nix develop --no-write-lock-file path:<flake_root>#<attr> --command <runtime argv>`.
- If the selected flake can be evaluated but none of the candidate dev shells exists, `run --dev-env auto`, `start --dev-env auto`, or `restart --dev-env auto` starts the runtime command directly.
- If automatic flake evaluation itself fails for reasons other than a missing candidate dev shell attribute, `run`, `start`, or `restart` fails clearly before starting or replacing a container.
