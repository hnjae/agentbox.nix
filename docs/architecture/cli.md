# CLI Boundary

The CLI boundary owns argument parsing, prompt rendering, stdout/stderr routing, exit codes, and conversion of domain failures into actionable user errors.

## Contracts

- Command implementations must preserve stdout for requested data output and inherited runtime/client stdout only; logs, progress, prompts, diagnostics, and application errors must use stderr.
- Parse errors remain owned by the CLI parser; application errors must use the shared diagnostic format.
