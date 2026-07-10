# Presentation And Observability Boundary

Presentation code must depend on domain results and errors; domain, discovery, runtime, image, and state layers must not depend on terminal rendering.

The presentation layer owns stderr terminal capability detection, terminal progress rendering, and diagnostic formatting. The runtime image lifecycle coordinator owns the progress task lifecycle around version resolution and image building, while runtime image environments and the Podman adapter expose operations and captured process output without depending on terminal rendering.

## Contracts

- Domain errors must carry the command, workspace or resource identity when recoverable, external command context when relevant, and a concise remediation hint when the spec defines one.
- Verbose diagnostics may reveal external commands and forwarded external command output, but they must not replace machine-readable stdout.
- Terminal progress is an stderr-only presentation concern. Interactive rendering requires a stderr TTY and a usable terminal, color capability is independent from interactivity, and every terminal progress task must clear itself before ordinary diagnostics or errors are emitted.
- Long-running work with unknowable completion must expose elapsed time and the current stage instead of a fabricated completion percentage.
- Non-interactive presentation must use stable line-oriented diagnostics without terminal control sequences. Verbose presentation must replace interactive progress with static stage diagnostics.
- Process execution may stream stdout and stderr to a presentation callback while retaining both streams for final results and failure diagnostics. The process layer owns concurrent draining so neither child stream can block the other; the callback owns only incremental presentation.
- Readiness and startup failures for managed containers must include bounded container log excerpts when Podman can provide them.
- Recovery guidance must prefer explicit user actions: `stop`, `stop --force`, `stop --all`, `start`, `restart`, `runtime update`, `clean`, or direct Podman removal only when a resource cannot be safely matched by agentbox.
