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

## Demo Recording

```sh
scripts/record-demo.sh
```

The script records an automated `start`, `ls`, `health`, `connect`, and `stop` flow with `asciinema` and writes the cast under `target/asciinema/` by default. The default Codex demo attaches to the managed session and sends `/exit` before stopping it; this mode requires `script(1)`. Use `--no-connect` to skip that step, `--connect` to enable it for another runtime, `--runtime opencode`, `--workspace PATH`, `--agentbox PATH`, or `--output PATH` to override the defaults. When `agg` and `ffmpeg` are available it also writes `@2x.gif` and `@2x.avif` media files next to the cast; use `--render-media` to require this step or `--no-render-media` to disable it.
