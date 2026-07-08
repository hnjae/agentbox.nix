# Host-Attached Nix

The OpenCode and Codex runtimes use host-attached Nix support inside the container alongside the workspace runtime cache volume.

Rules:

- `/nix` is mounted into the container so the host Nix store and nix-daemon socket are available.
- `NIX_REMOTE=daemon` and the daemon socket at `/nix/var/nix/daemon-socket/socket` are part of the runtime contract.
- A host-compatible `nix` client is available in `PATH` inside the runtime.
- `/etc/nix` is mounted so host configuration and registry inheritance are visible inside the container.
- `/etc/static/nix` is mounted only when needed because `/etc/nix` resolves there on the host model.
- If a file under `/etc/nix`, such as `/etc/nix/nix.custom.conf`, points into `/etc/static/nix`, `run`, `start`, or `restart` treats `/etc/static/nix` as needed even when that static file ultimately resolves into `/nix/store`.
- Runtime profile state lives under `$XDG_STATE_HOME/nix/profile`, with fallback to `$HOME/.local/state/nix/profile` or `/home/user/.local/state/nix/profile` when needed.
- The runtime image provides its own CA bundle. Host SSL trust-store mounts are unsupported.
- If a host-attached Nix prerequisite is missing, `run`, `start`, or `restart` fails clearly and does not attempt to synthesize a bundled Nix installation.
