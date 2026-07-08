# `agentbox stop [target]...` / `agentbox stop --all`

`stop` stops managed workspace sessions or transient `run` containers for the resolved repositories, exact stored git-root absolute paths, or stable id prefixes. It is not a volume pruning command. With `--all`, `stop` stops every running managed or transient `run` agentbox-owned container.

Expected behavior:

1. If one or more `<target>` values are present, process each target and continue to later targets after a target-specific failure.
2. For each `<target>` that names an existing path, resolve it to a canonical git root.
3. For each `<target>` that is an absolute path that does not exist, require it to exactly match a live managed session or transient `run` container's stored `io.agentbox.git_root` absolute path string. This selector may match orphaned sessions and failed resources that still have a recoverable stored git-root path.
4. For each `<target>` that is not resolved as a path, treat it as a stable id prefix. Prefix matching is case-insensitive.
5. If no session id matches a target prefix, record a clear failure for that target.
6. If a prefix matches more than one distinct id, record a failure asking for a longer prefix.
7. If all prefix matches have the same full id, treat them as duplicate sessions for that id.
8. Ensure concurrent lifecycle operations for the same git root do not race.
9. Stop each matching container if it is running.
10. Treat an already-removed matching container as success after verifying it is absent.
11. If no matching agentbox container exists for a target, record that no resource exists for the resolved repository or exact stored git-root path.
12. After all explicit targets are processed, exit non-zero if any target failed or any cleanup verification failed, and include a summary of the failed targets.
13. Rely on Podman's `--rm` cleanup for container removal after the stop.
14. Leave the runtime cache volume unmanaged by `stop` so it can be reclaimed later by explicit Podman volume cleanup.
15. If no `<target>` is present and `--all` is not set, discover stop candidates and prompt on stderr with a fuzzy multi-select list.
16. The no-target selector includes running, orphaned, duplicate, and failed managed sessions and transient `run` containers when a stable id is known, matching stop completion eligibility.
17. If the no-target selector is canceled with Escape, exit non-zero with `selection canceled`.
18. If the no-target selector is interrupted with Ctrl-C, exit non-zero with `selection interrupted`.
19. If no `<target>` is present and either stdin or stderr is not a terminal, fail with a clear error that a stop target or `--all` is required in non-interactive use.
20. If no selector candidates exist, print `agentbox stop: no agentbox containers available to stop` as an `INFO` log on stderr and exit successfully without stopping anything.
21. If the selector returns an empty selection, print `agentbox stop: no sessions selected` as a `WARNING` log on stderr and exit successfully without stopping anything.
22. If `--all` is set, do not accept a `<target>` and stop all running managed sessions and transient `run` containers discovered from live Podman state.
23. `stop --all` stops running, orphaned, duplicate, and otherwise malformed managed or transient `run` containers whose Podman state is running.
24. `stop --all` ignores agentbox containers that are already stopped.
25. If `stop --all` finds no running agentbox containers, exit successfully.
26. For `stop --all`, lock each recoverable git root before stopping its currently running exact matches. Running agentbox containers without a recoverable git-root label are stopped only because the user selected the explicit global cleanup.

Optional flag:

- `--force`: best-effort cleanup when duplicate or failed exact matches exist
- `--all`: stop every running managed or transient `run` agentbox container

Safety rules:

- Without `--force`, `stop` fails when more than one matching agentbox container is found.
- With `--force`, `stop` stops all live agentbox containers that exactly claim the resolved canonical git root, exact stored git-root path, or selected stable id. It still does not stop containers that cannot be matched to that identity.
- With multiple explicit targets, `stop` may stop sessions for successful targets even when other targets fail.
- `--force` is not required with `--all`; `--all` already selects every running managed or transient `run` agentbox container.
- Stable id matching includes failed sessions. When a matched session has a recoverable git-root label, `stop` uses that git root for locking; when only a stable id is recoverable, `stop` may stop only exact live matches for that id and must not expand the selection to unrelated containers.
- `stop` never deletes the user workspace.
- `stop` never directly removes images or named cache volumes.
- Stop status and no-op messages are stderr logs. Successful `stop` does not write to stdout.
