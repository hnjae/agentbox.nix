# Host-attached Nix development container

This repository builds a development container that uses a host-provided Nix
installation instead of bundling its own.

## Supported host model

- NixOS
- Linux with multi-user Nix

The container intentionally inherits the host `/etc/nix` configuration,
including registry behavior.

## Runtime contract

The container expects an image-local CA bundle at runtime from the Debian
`ca-certificates` package.

The container also expects these host-provided pieces to be mounted at runtime:

- `/nix` for the host store and nix-daemon socket
- a host `nix` client in `PATH`
- `/etc/nix` for host config and registry inheritance

On NixOS, `/etc/nix` may resolve through `/etc/static/nix`. That path is a
conditional resolution detail, not a universal required mount.

Initial container startup runs the image in bootstrap mode. That one-time path
checks the host-attached prerequisites, uses the Podman-volume-backed
`/home/user/.cache/nix` mount for cache and eval artifacts, materializes the
active `runtime` profile under `/home/user/.local/state/nix/profile` with
`nix profile add`, and then hands off to `/entrypoint` for the final command.

The `agentbox` CLI tags default runtime images with a content hash for the
embedded image build context. OpenCode images install OpenCode from npm package
`opencode-ai`, and Codex images install Codex from npm package
`@openai/codex`, at the version selected by the image build. The container
defaults to the non-root `user` account with `HOME=/home/user`.

For later `podman exec` calls, use `/entrypoint <cmd>` as the supported
interface. `/entrypoint` re-checks the runtime contract and rebuilds
`NIX_SSL_CERT_FILE`, `NIX_PROFILES`, `PATH`, and `XDG_DATA_DIRS` before it
executes the requested command. Plain `podman exec ... zsh` is not the
guaranteed contract; it only works when the invoked shell rebuilds the same
environment on its own.

## Startup-path validation

Bootstrap validates the real startup path by running `nix profile add` against
the `runtime-packages.nix` manifest with the host-mounted client and inherited
host config, while `/entrypoint` is the reusable later-exec wrapper for
per-process environment activation.

## Non-goals

- providing a standalone bundled Nix installation inside the image
- supporting host models outside NixOS or Linux with multi-user Nix
- replacing the host registry or `/etc/nix` behavior with container-local state
