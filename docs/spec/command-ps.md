# `agentbox ps`

`ps` lists agentbox-owned managed workspace sessions and transient `run` containers from live Podman discovery.

Expected output fields:

- id, or `unknown`
- type
- canonical git root, or `unknown`
- runtime, or `unknown`
- status
- endpoint, or `unknown`

Type values:

- `managed`: a managed workspace session.
- `run`: a transient `run` container.

Status values:

- `running`: the agentbox container exists and is running.
- `orphaned`: the agentbox container exists and is running, but the stored git root path no longer exists on the host.
- `duplicate`: more than one agentbox container claims the same canonical git root.
- `failed`: the agentbox container exists, but required metadata, workspace mounts, published endpoint data, or other inspectable session invariants are inconsistent.

Rules:

- Containers not marked as managed sessions or transient `run` containers by `agentbox` are ignored, even if their names resemble `agentbox` names.
- The public session id is the stable 12-character value from the `io.agentbox.git_root_hash` label. It is not the Podman container id.
- For `failed` sessions, fields that cannot be recovered from live Podman state are shown as `unknown`.
- By default, `ps` prints a compact borderless human-readable table.
- `ps --output table` and `ps -o table` explicitly select the same table output.
- `ps --output json`, `ps --output=json`, and `ps -o json` print a compact single-line JSON array followed by a newline.
- JSON rows contain stable keys: `id`, `type`, `canonical_git_root`, `runtime`, `status`, `endpoint`, and `container_name`.
- JSON keeps `container_name` for automation even though the table omits it.
- JSON uses `null` for unrecoverable `id`, `canonical_git_root`, `runtime`, or `endpoint` values instead of the table's `unknown` placeholder.
- JSON rows use the same ordering as table rows.
- Table output uses uppercase headers, no leading or trailing table padding, and ends with a newline.
