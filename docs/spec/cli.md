# CLI

Global flags:

- `--verbose` enables diagnostic command traces and external command output for commands that support verbose diagnostics. Diagnostic output is written to stderr and must not replace machine-readable or success output on stdout.

Global output rules:

- stdout is reserved for user-requested data output: `ps`, `health`, shell completion output, hidden completion root output, `--help`, and `--version`. Status messages, progress, success summaries, cancellation notices, verbose traces, forwarded external command output, and application errors are written to stderr.
- Application stderr logs use one line per message with this shape: `[2026-05-06T22:15:56+09:00] INFO: message`.
- Log timestamps use the local UTC offset. If the local offset cannot be determined, timestamps use UTC with `+00:00`.
- Log severities are `ERR`, `WARNING`, `INFO`, and `DEBUG`.
- ANSI color is used only when stderr is a TTY and `NO_COLOR` is not set. Timestamps and `DEBUG` labels are bright black, `ERR` labels are red, `WARNING` labels are yellow, and `INFO` labels are blue. In the `selected development environment` info log, the selected provider name or `none` is bold bright cyan.
- Clap parse errors and usage text keep Clap's native stderr format. `--help` and `--version` keep Clap's native stdout format.
- Interactive prompt UI, including context text rendered specifically for a prompt, is rendered on stderr without being wrapped as log lines. `connect` runs the runtime host client with inherited stdio and does not wrap the client output as logs.
