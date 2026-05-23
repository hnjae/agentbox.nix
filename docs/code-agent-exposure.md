# Code Agent Exposure Surface

This document describes what a code-agent runtime launched by `agentbox` can see, mutate, or reach in the current implementation. It is an implementation guide, not a hardening guarantee; `agentbox` relies on rootless Podman isolation plus explicit mounts, explicit environment variables, and runtime-specific attach protocols.

## Process Boundary

`agentbox run`, `agentbox start`, `agentbox restart`, and `agentbox exec` run the selected runtime command in a Podman container. `agentbox run`, `agentbox start --connect`, `agentbox restart --connect`, and `agentbox connect` also run the selected runtime host client directly on the host with inherited stdio and a host working directory. `run` starts the host client from the canonical target directory; `start --connect`, `restart --connect`, and `connect` start it from the managed session's stored launch directory. That host client is not containerized by `agentbox`; it has the normal host access of the invoking user and whatever access the runtime client itself performs outside the remote protocol.

Inside the container, `agentbox` starts the process as the image-local `user` account with UID `1000`, maps the invoking host user's primary GID through `--userns keep-id`, and preserves supplemental groups with `--group-add keep-groups`. Host ownership and permission bits remain authoritative for bind mounts. `agentbox` does not pass `--privileged`, mount the Podman socket, or mutate host permissions to force access.

Codex server, exec, and host remote-client commands are launched with `--dangerously-bypass-approvals-and-sandbox`, so Codex's own approval and sandbox layer is disabled for those Codex processes. `agentbox exec` also disables `codex_git_commit`. OpenCode containers receive `OPENCODE_PERMISSION={"*":"allow"}`.

## Filesystem

The container receives these filesystem entries when the corresponding launch mode applies:

| Host source | Container path | Access | Applies to |
| --- | --- | --- | --- |
| Canonical git root | Same absolute path | Read-write bind mount | `run`, `start`, `restart`, `exec` |
| Workspace runtime cache volume | `/home/user` | Read-write Podman named volume | `run`, `start`, `restart`, `exec` |
| `/nix` | `/nix` | Read-only bind mount | `run`, `start`, `restart`, `exec` |
| Host `nix` client path | `/usr/local/bin/nix` | Read-only bind mount | `run`, `start`, `restart`, `exec` |
| `/etc/nix` | `/etc/nix` | Read-only bind mount | `run`, `start`, `restart`, `exec` |
| `/etc/static/nix` | `/etc/static/nix` | Read-only bind mount | Only when host `/etc/nix` resolution requires it |
| Host Codex home | `/home/user/.codex` or the same absolute `CODEX_HOME` path | Read-write bind mount | Codex launches |
| Host OpenCode config | `/home/user/.config/opencode` | Read-write bind mount | OpenCode server launches |
| Host OpenCode data | `/home/user/.local/share/opencode` | Read-write bind mount | OpenCode server launches |
| Host Git excludes file | `/run/agentbox/git-ignore` | Read-only bind mount | Launches with an existing readable host Git excludes file |
| Host SSH agent socket | `/run/agentbox/ssh-agent.sock` | Unix socket bind mount | Launches with usable host `SSH_AUTH_SOCK` |
| Temporary known_hosts file | `/run/agentbox/known_hosts` | Read-only bind mount | Launches with prepared SSH known_hosts entries |

The workspace mount exposes the whole canonical git root, not only the command's target subdirectory. The runtime working directory is the canonical target directory for `run`, `start`, and `exec`; `restart` preserves the managed session's stored launch directory.

The `/home/user` volume persists per workspace identity and stores runtime home state, Nix cache/evaluation artifacts, the runtime Nix profile, and any other files the runtime writes under `/home/user` unless a more specific passthrough mount shadows that path.

For Codex server launches, a non-empty `CODEX_HOME` selects the host directory to expose. That directory must be an existing absolute directory and is mounted at the same absolute path inside the container, with `CODEX_HOME` set to the same value in the container. If `CODEX_HOME` is unset or empty, `${HOME}/.codex` is mounted at `/home/user/.codex`. `agentbox exec` currently ignores `CODEX_HOME` and always uses `${HOME}/.codex` at `/home/user/.codex`.

For OpenCode server launches, `${XDG_CONFIG_HOME:-$HOME/.config}/opencode` and `${XDG_DATA_HOME:-$HOME/.local/share}/opencode` are mounted read-write into the container.

`agentbox` does not mount the host home directory as a whole, `~/.ssh`, private keys, complete Git config files, credential helper state, Docker or Podman sockets, or the `agentbox` state directory by default.

## Nix

The runtime uses host-attached Nix. The container sees the host Nix store at `/nix`, the nix-daemon socket at `/nix/var/nix/daemon-socket/socket`, the host Nix client mounted into `PATH`, and host `/etc/nix` configuration. The `/nix` bind mount is read-only, but `NIX_REMOTE=daemon` is set by the image, so Nix operations are requests to the host daemon and may realize or add store paths according to the host daemon's normal authorization and configuration.

The runtime image provides a Debian base, the selected runtime package installed from npm, and an activated Nix profile containing runtime tools such as `gh`, `just`, `devenv`, `direnv`, `nix-direnv`, `ast-grep`, `yq`, and `comment-checker`.

## Git And SSH

For `run`, `start`, and `restart`, `agentbox` reads only the effective host repository `user.name` and `user.email` and injects them with Git's `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_*`, and `GIT_CONFIG_VALUE_*` environment variables. For `exec`, it injects `user.name=Codex` and `user.email=noreply@openai.com` instead.

For `run`, `start`, `restart`, and `exec`, `agentbox` also passes an existing readable host Git excludes file when the repository has an effective `core.excludesFile`, or when Git's default `${XDG_CONFIG_HOME:-$HOME/.config}/git/ignore` exists. The file is mounted read-only at `/run/agentbox/git-ignore`, and `core.excludesFile=/run/agentbox/git-ignore` is injected with the same `GIT_CONFIG_COUNT` environment mechanism. Missing excludes files are not mounted and do not block launch.

If host `SSH_AUTH_SOCK` points to a usable Unix socket, `agentbox` bind-mounts that socket at `/run/agentbox/ssh-agent.sock` and sets `SSH_AUTH_SOCK=/run/agentbox/ssh-agent.sock` inside the container. The container can ask the host agent to sign with keys loaded in that agent, but the host private key files are not mounted.

When SSH agent passthrough is active, `agentbox` may also inject SSH commit-signing Git config values if the effective host repository config uses SSH signing. It may prepare a temporary strict known_hosts file for SSH Git remotes and set `GIT_SSH_COMMAND` to use it. It does not mount complete host known_hosts files or host SSH config.

## Environment

`agentbox` does not request whole-host environment passthrough for the container. The container receives image-defined environment such as `HOME=/home/user`, `NIX_REMOTE=daemon`, and `NIX_DAEMON_SOCKET_PATH=/nix/var/nix/daemon-socket/socket`, plus explicit launch environment entries generated by `agentbox` for runtime defaults, Codex `CODEX_HOME` passthrough, Git identity, Git excludes file passthrough, SSH agent passthrough, and SSH host verification. Podman may still apply its own defaults, such as proxy environment propagation, according to the host Podman configuration.

The image entrypoint also derives runtime environment such as `XDG_CACHE_HOME`, `XDG_STATE_HOME`, `NIX_SSL_CERT_FILE`, `NIX_PROFILES`, `PATH`, and `XDG_DATA_DIRS`. With `--dev-env auto`, the selected development-environment wrapper (`direnv`, `devenv`, or `nix develop`) runs inside the container and can add environment according to the workspace's checked-in configuration and host-attached Nix behavior.

The Codex attach capability token itself is not passed to the container server. The server receives only the token SHA-256 as a command argument. For managed Codex sessions, the full token is stored under the host `agentbox` state directory and provided only to the host-side Codex remote client through `AGENTBOX_CODEX_REMOTE_TOKEN`.

## Network

Container networking is enabled for runtime containers. Outbound network access and access to other addresses reachable from the container follow Podman's default networking and the host firewall or network policy.

Server runtimes listen on `0.0.0.0` inside the container so Podman's port forwarding can reach them. `agentbox` publishes server attach endpoints only on host loopback, using `127.0.0.1::<container-port>` and a Podman-assigned host port. Foreground `agentbox exec` does not publish an attach endpoint.

Codex server attach uses WebSocket capability-token authentication. OpenCode attach relies on the runtime's attach behavior plus the host-loopback-only Podman publication.
