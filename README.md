# agentbox.nix

> [!IMPORTANT]
> `agentbox` is pre-release. Behavior, flags, runtimes, and state formats may change.

`agentbox` runs code agents in isolated Podman containers for a git workspace.

It is built to:

- Let containers use host Nix safely via read-only `/nix` and the host nix-daemon.
- Run Nix-aware environments such as `devenv` and `nix develop` inside the container.
- Leverages server-client code agent with containerized runtime servers and host-side clients.

## Features

- Runs Codex or OpenCode in isolated, workspace-scoped Podman containers.
- Detects `direnv`, `devenv`, and `nix develop`.
- Manages agentbox-owned containers, images and volumes.

## Documentation

- [Code agent exposure surface](docs/code-agent-exposure.md): host paths, environment, network access, and host-side processes visible to runtime agents.

## Install

```sh
nix profile add 'github:hnjae/agentbox.nix#agentbox'
```

Requires Podman, Git, and host Nix with a running nix-daemon.

## Usage

```sh
# Run and connect immediately
agentbox run --runtime codex /path/to/workspace
agentbox run --runtime opencode /path/to/workspace

# Start a detached session
agentbox start --runtime codex /path/to/workspace
agentbox start --runtime opencode /path/to/workspace

# Connect to a managed session
agentbox connect /path/to/workspace

# Run a one-shot Codex job
agentbox exec /path/to/workspace -- "review this change"

# Manage resources
agentbox ls
agentbox health
agentbox restart /path/to/workspace
agentbox stop /path/to/workspace
agentbox clean
```
