# Agentbox Specification

This directory contains user-visible CLI behavior and operator-visible runtime state for the `agentbox` CLI, installed package, managed Podman objects, and filesystem effects. It excludes Rust module structure and private implementation details.

- [Overview](overview.md): product summary, supported environments, required tools, configuration, resource limits, and MVP scope.
- [Workspace Identity](workspace-identity.md): git-root resolution, session identity, deterministic naming, visible Podman state, and ownership labels.
- [Commands](commands.md): global CLI rules and command behavior for `run`, `exec`, `start`, `restart`, `clean`, `runtime update`, `connect`, `ps`, `health`, and `stop`.
- [Installed Assets](installed-assets.md): shell completion behavior and installed manual/completion asset paths.
- [Runtime Filesystem](runtime-filesystem.md): workspace mounts, runtime cache volume, Git and SSH passthrough, runtime host state, development environment loading, server/client behavior, and host-attached Nix.
- [Lifecycle And Errors](lifecycle-and-errors.md): lifecycle model, drift recovery, and required error cases.
- [Security](security.md): isolation expectations, runtime user behavior, bind-mount permission rules, and security out-of-scope items.
