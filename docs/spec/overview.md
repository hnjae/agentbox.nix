# Overview

This document specifies user-visible CLI behavior and operator-visible runtime state for the `agentbox` CLI, installed package, managed Podman objects, and filesystem effects. It excludes Rust module structure and private implementation details.

## Summary

`agentbox` is a Rust CLI for running code agents inside isolated Podman containers.

The MVP is workspace-centric:

- `agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-client-args>...]`
- `agentbox exec [--dev-env <auto|none>] [directory] [-- <codex-exec-args>...]`
- `agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-server-args>...]`
- `agentbox runtime update <opencode|codex>`
- `agentbox connect [directory] [-- <agent-client-args>...]`
- `agentbox ps`
- `agentbox health`
- `agentbox stop [target]...`
- `agentbox clean`

`agentbox run [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-client-args>...]` resolves `[directory]` to its canonical git root, starts a transient runtime server container, and connects with the selected host client from the canonical target directory. If `[directory]` is omitted, it defaults to `.`. The transient container is agentbox-owned and visible to `ps` and `stop`, but not a managed session for `connect` or `health`. `--dev-env auto` wraps the server command in the applicable development environment; `--dev-env none` runs it directly. `--cpus` and `--memory` set optional container resource limits for the server container. Interactive use prompts for `--runtime` when omitted. Agent client arguments must follow `--` and are appended to the host client command only.

`agentbox exec [--dev-env <auto|none>] [directory] [-- <codex-exec-args>...]` resolves `[directory]` to its canonical git root and runs `codex exec` once in a foreground Podman container. If `[directory]` is omitted, it defaults to `.`. It is not managed and is not listed or selectable by lifecycle commands. `--dev-env auto` wraps `codex exec` in the applicable development environment; `--dev-env none` runs it directly. Codex exec arguments must follow `--`.

`agentbox start [--connect|-c] [--runtime <opencode|codex>] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [directory] [-- <agent-server-args>...]` resolves `[directory]` to its canonical git root and starts one managed detached runtime server session for that repository. If `[directory]` is omitted, it defaults to `.`. `--dev-env auto` wraps the server command in the applicable development environment; `--dev-env none` runs it directly. `--cpus` and `--memory` set optional container resource limits for the server container. `--connect` attaches with the host client after readiness. Agent server arguments must follow `--`, are appended to the runtime server command, and are preserved for `restart`; they are not passed to the host client launched by `--connect`.

`agentbox restart [--connect|-c] [--dev-env <auto|none>] [--cpus <cpus>] [--memory <memory>] [target]` replaces one running managed session for the same repository. It recovers runtime, launch directory metadata, stored agent server arguments, and stored resource limits, reuses the named cache volume, and re-evaluates development environment loading for the stored launch directory.

`agentbox connect [directory] [-- <agent-client-args>...]` discovers the running server endpoint for the resolved repository or selected session and runs the matching host client from the stored launch directory. A provided directory selects the workspace only; it does not change the running session's working directory. Interactive use prompts when the directory is omitted. Agent client arguments must follow `--` and are appended to the host client command only.

Foreground `exec`, transient `run`, and managed containers use Podman's `--rm` cleanup flag. Podman removes stopped containers after foreground exit, transient stop, managed server exit, `agentbox stop`, or the stop phase of `agentbox restart`. Default runtime images and named runtime cache volumes remain for explicit cleanup or update.

If a matching managed session exists, `run`, `exec`, and `start` fail instead of reusing, replacing, or mutating it. If a matching transient `run` exists, `run` and `start` fail before creating another container and suggest `agentbox stop <id>`.

MVP runtime support includes OpenCode and Codex.

## Supported Environment And Limits

Supported host environments:

- NixOS
- other Linux distributions with multi-user Nix

Always-required host tools:

- Podman
- Git
- a host `nix` client and nix-daemon socket compatible with the host-attached Nix model described below

Conditionally required host tools:

- the selected runtime host client command for `run`, and the running session's runtime host client command for `connect` and `start --connect`
- `npm` when `agentbox` must resolve the latest runtime npm package version for initial default image creation, a default runtime image rebuild after the embedded image build context changes, or `agentbox runtime update <runtime>`

User configuration is read from `${XDG_CONFIG_HOME:-$HOME/.config}/agentbox/config.json` as strict JSON. Supported top-level fields are `knownHosts` and `defaultResourceLimits`; both fields are optional and unknown fields are incompatible. The repository root `config.sample.json`, also installed at `share/doc/agentbox/config.sample.json`, is the reference sample for supported fields. `knownHosts` is an array of single-line SSH known-host entries. `defaultResourceLimits` may contain `cpus` and `memory` defaults for server containers launched by `run`, `start`, and `restart`. Invalid config is warned about, moved aside to a timestamped `config.json.bak...` path when possible, and ignored for the current invocation.

Server container CPU and memory limits use Podman's `--cpus` and `--memory` options. `cpus` is a non-negative decimal CPU count, and `0` means unlimited. `memory` uses Podman's `<number>[b|k|m|g]` memory format, and `0` means unlimited. Limits resolve independently. For `run` and `start`, each CLI limit overrides the corresponding config default, then falls back to unlimited. For `restart`, each CLI limit overrides the corresponding stored managed-session limit, then the config default for older unlabeled sessions, then unlimited.

For `run`, `exec`, `start`, and `connect`, a provided directory must resolve to an existing directory inside a git repository. For `run`, `exec`, and `start`, omitting the directory is equivalent to passing `.`. A non-git target fails clearly; the MVP does not create ad-hoc non-git sessions. `stop` normally follows the same resolution rules, but it may also accept an exact stored git-root absolute path string for a recoverable session whose stored path no longer exists.

Out of scope for the MVP:

- macOS or Windows support
- more than one valid managed session for the same canonical git root
- silent runtime switching or automatic recreation when an existing managed session has invalid runtime metadata
- user-supplied runtime image references
- generic container orchestration
- durable runtime state beyond the workspace bind mount, one Podman-managed runtime cache volume, Codex host configuration passthrough, OpenCode host state passthrough, runtime image version metadata, Codex managed-session attach tokens, and live managed-container metadata
- a bundled standalone Nix installation inside the runtime image
- stopped managed containers that remain after the runtime server exits
