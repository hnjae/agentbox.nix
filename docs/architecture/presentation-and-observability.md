# Presentation And Observability Boundary

Presentation code must depend on domain results and errors; domain, discovery, runtime, image, and state layers must not depend on terminal rendering.

## Contracts

- Domain errors must carry the command, workspace or resource identity when recoverable, external command context when relevant, and a concise remediation hint when the spec defines one.
- Verbose diagnostics may reveal external commands and forwarded external command output, but they must not replace machine-readable stdout.
- Readiness and startup failures for managed containers must include bounded container log excerpts when Podman can provide them.
- Recovery guidance must prefer explicit user actions: `stop`, `stop --force`, `stop --all`, `start`, `restart`, `runtime update`, `clean`, or direct Podman removal only when a resource cannot be safely matched by agentbox.
