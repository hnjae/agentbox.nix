# `agentbox health [target]`

`health` reports runtime health for currently running managed workspace sessions from live Podman discovery. Without a target, it probes every running session. With a target, it probes one running session selected by stable id prefix or workspace directory.

Expected output fields:

- id
- canonical git root
- runtime
- health
- reason
- endpoint

Health values:

- `healthy`: the runtime's official health endpoint responded successfully.
- `unhealthy`: the runtime's official health endpoint did not respond with the runtime-specific healthy result, or required runtime/endpoint metadata was not recoverable from the running session.

Runtime probes:

- OpenCode is probed with `GET /global/health` on the discovered attach endpoint. The session is healthy only when the response is `HTTP 200` and the JSON response body has `healthy: true`.
- Codex is probed with `GET /readyz` on the discovered attach endpoint. The session is healthy only when the response is `HTTP 200`.

Rules:

- `health` includes only sessions whose discovered session status is `running`.
- Failed, stopped, orphaned, and duplicate sessions are not included.
- Transient `run` containers are not included or probed.
- `health` probes each running session once and does not wait for recovery.
- `health <target>` treats an existing path as a workspace directory and resolves it to a canonical git root.
- `health <target>` treats a missing relative path or other non-path target as a stable id prefix. Prefix matching is case-insensitive.
- If no running managed session matches the target, `health <target>` fails clearly.
- If the target matches more than one distinct id or workspace session, `health <target>` fails and asks for a more specific target.
- If the selected session is not `running`, `health <target>` fails clearly instead of probing it.
- By default, `health` prints a compact borderless human-readable table.
- `health --output table` and `health -o table` explicitly select the same table output.
- `health --output json`, `health --output=json`, and `health -o json` print a compact single-line JSON array followed by a newline.
- JSON rows contain stable keys: `id`, `canonical_git_root`, `runtime`, `health`, `reason`, `endpoint`, and `container_name`.
- JSON keeps `container_name` for automation even though the table omits it.
- JSON uses `null` for unrecoverable `id`, `canonical_git_root`, `runtime`, or `endpoint` values instead of the table's `unknown` placeholder.
- JSON rows use the same ordering as table rows.
- Table output uses uppercase headers, no leading or trailing table padding, and ends with a newline.
- A healthy row uses reason `ok`.
- An unhealthy row uses a concise reason such as `unreachable`, `HTTP 503`, `malformed JSON`, or `healthy=false`.
- If there are no running sessions, `health` prints an empty table with headers by default, prints `[]` in JSON mode, and exits `0`.
- Unhealthy rows do not make the command fail; discovery or Podman failures remain command failures.
