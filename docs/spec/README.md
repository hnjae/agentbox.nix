# Agentbox Specification

This directory contains user-visible CLI behavior and operator-visible runtime state for the `agentbox` CLI, installed package, managed Podman objects, and filesystem effects. It excludes Rust module structure and private implementation details.

## Index

- [Overview](overview.md): product summary, supported environments, required tools, configuration, resource limits, and supported or unsupported scope.
- [CLI](cli.md): global flags, stdout/stderr rules, logging, color, parse errors, and prompt output.
- [Workspace Identity](workspace-identity.md): git-root resolution, session identity, deterministic naming, visible Podman state, and ownership labels.
- [Run Command](command-run.md): `agentbox run`.
- [Exec Command](command-exec.md): `agentbox exec`.
- [Start Command](command-start.md): `agentbox start`.
- [Restart Command](command-restart.md): `agentbox restart`.
- [Connect Command](command-connect.md): `agentbox connect`.
- [PS Command](command-ps.md): `agentbox ps`.
- [Health Command](command-health.md): `agentbox health`.
- [Stop Command](command-stop.md): `agentbox stop`.
- [Clean Command](command-clean.md): `agentbox clean`.
- [Runtime Update Command](command-runtime-update.md): `agentbox runtime update`.
- [Default Runtime Images](default-runtime-images.md): default runtime image identity, setup, reuse, metadata, and wrapper output.
- [Runtime Workspace](runtime-workspace.md): workspace mounts, launch directory working directory, runtime cache volume, and runtime profile activation.
- [Host Passthrough](host-passthrough.md): Git identity, Git excludes, SSH signing, known-host verification, Codex host configuration, and OpenCode host state.
- [Development Environment](development-environment.md): automatic `direnv`, `devenv`, and `nix develop` wrapper selection.
- [Runtime Connectivity](runtime-connectivity.md): runtime server/client behavior, readiness probes, attach endpoints, proxy handling, and Codex attach tokens.
- [Host-Attached Nix](host-attached-nix.md): host Nix mounts, daemon access, runtime package installation, CA bundle behavior, and prerequisite failures.
- [Installed Assets](installed-assets.md): shell completion behavior and installed manual/completion asset paths.
- [Lifecycle And Errors](lifecycle-and-errors.md): lifecycle model, drift recovery, and required error cases.
- [Security](security.md): isolation expectations, runtime user behavior, bind-mount permission rules, and security out-of-scope items.
