# `agentbox clean`

`clean` reclaims unused Podman resources and lock files owned by `agentbox`. It is a global cleanup command, not a session stop command.

Optional flags:

- `--dry-run`: print cleanup candidates and skipped resources without deleting anything
- `--yes`: delete cleanup candidates without prompting
- `--images`: consider default runtime images only
- `--volumes`: consider workspace cache volumes only
- `--locks`: consider workspace lock files only

Selection rules:

- If none of `--images`, `--volumes`, or `--locks` is set, `clean` considers default runtime images, workspace cache volumes, and workspace lock files.
- If any of `--images`, `--volumes`, or `--locks` is set, only the selected resource kinds are considered.
- `--dry-run` and `--yes` cannot be used together.

Image cleanup rules:

- `clean` considers agentbox-owned default runtime images discovered from image labels, including old content-hash-tagged default image references. Image names or references alone do not make an image eligible for cleanup unless the image also carries agentbox default-runtime-image ownership labels.
- A default runtime image candidate is skipped when any Podman container, managed or unmanaged, currently uses that exact image reference.
- When the current state file for a runtime points to an image that was deleted successfully, the corresponding runtime image metadata file under `$XDG_STATE_HOME/agentbox/runtime` is removed. If state points to a different image that is still present or in use, it is preserved.
- `clean` does not remove image names by prefix and does not call `podman system prune` or Podman build-cache cleanup.

Volume cleanup rules:

- `clean` only considers named volumes whose names match the workspace cache volume shape `agentbox-...-<12 hex>`.
- A candidate volume is skipped when any Podman container, managed or unmanaged, mounts that exact named volume source. Bind mounts whose host source path happens to match the volume name do not count as volume usage.
- `clean` does not call broad Podman volume pruning such as `podman volume prune --all`.

Lock file cleanup rules:

- `clean` only considers lock files whose paths match the workspace lock file shape `$XDG_STATE_HOME/agentbox/locks/<64 lowercase hex>.lock`.
- A candidate lock file is skipped when `agentbox` cannot acquire its workspace lock without blocking, which indicates that another process currently holds the lock.
- Files in the lock directory that do not match the workspace lock file name shape, symlinks, directories, and lock-like files outside the current workspace lock directory are ignored.

Confirmation and output rules:

- If no resources are cleanup candidates and no resources are skipped, `clean` emits an `INFO` log `nothing to clean` on stderr and exits successfully.
- If resources are skipped but no resources are cleanup candidates, `clean` emits the skip reasons on stderr and exits successfully without prompting.
- With `--dry-run`, `clean` emits cleanup candidates and skip reasons as `INFO` logs on stderr, deletes nothing, and exits successfully.
- With `--yes`, `clean` deletes cleanup candidates without prompting.
- Without `--dry-run` or `--yes`, `clean` renders cleanup candidates and skip reasons as prompt context followed by an interactive confirmation prompt on stderr only when stdin and stderr are terminals. The prompt context is not wrapped as log lines. The default answer is No. Case-insensitive `y` or `yes` approves cleanup; `n`, `no`, an empty response, or prompt cancellation emits a `WARNING` log `aborted` on stderr and exits successfully.
- Interrupting the confirmation prompt with Ctrl-C exits non-zero with `confirmation interrupted`.
- When stdin or stderr is not a TTY, `clean` fails unless `--yes` or `--dry-run` is set.
- If deletion of one candidate fails, `clean` continues deleting the remaining candidates, then exits non-zero with a summary of failed resources.
- Cleanup candidate and skip messages are stderr logs for `--dry-run`, `--yes`, skipped-only, and no-op reporting paths; in the interactive confirmation path, cleanup candidates and skip reasons are prompt context instead. Deletion, abort, and no-op messages are stderr logs. Successful `clean` does not write to stdout.

Safety rules:

- `clean` never stops or removes running or stopped containers. Container lifecycle remains owned by `agentbox stop`.
- `clean` never deletes a workspace, the Nix store, `~/.codex`, or host OpenCode configuration or state directories.
