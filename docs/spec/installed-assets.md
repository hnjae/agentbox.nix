# Installed Assets

## Completion And Installed Assets

Shell completion for `connect`, `restart`, `stop`, and `health` is dynamic.

Required behavior:

- Completion candidates come from live agentbox containers, not from a static file.
- `connect` candidate values are canonical or stored git root paths when known. Sessions with no recoverable git-root path are not connect completion candidates, but remain visible through `agentbox ps`.
- `connect` completion includes only connectable `running` managed sessions with valid endpoint metadata.
- `restart` candidate values are stable ids.
- `restart` completion includes only running managed sessions with recoverable runtime and launch-directory metadata.
- Transient `run` containers are not `connect`, `restart`, or `health` completion candidates.
- `stop` and `health` candidate values are stable ids.
- `stop` completion includes running, orphaned, duplicate, and failed sessions and transient `run` containers when a stable id is known.
- `stop` completion offers stable id candidates at every target position, not only the first target position.
- `health` completion includes running managed sessions when a stable id is known.
- Candidate descriptions include root, runtime, and status when the shell supports descriptions.
- Eligible live sessions are reflected immediately at tab completion time.
- `fzf-tab`-style frontends work automatically because they consume normal shell completion results.

The default Nix package installs shell completion and manual assets alongside the `agentbox` binary.

Required package output paths:

- `share/bash-completion/completions/agentbox`
- `share/zsh/site-functions/_agentbox`
- `share/fish/vendor_completions.d/agentbox.fish`
- `share/doc/agentbox/config.sample.json`
- `share/man/man1/agentbox.1`, `share/man/man1/agentbox-run.1`, `share/man/man1/agentbox-exec.1`, `share/man/man1/agentbox-start.1`, `share/man/man1/agentbox-restart.1`, `share/man/man1/agentbox-connect.1`, `share/man/man1/agentbox-ps.1`, `share/man/man1/agentbox-health.1`, `share/man/man1/agentbox-stop.1`, `share/man/man1/agentbox-clean.1`, `share/man/man1/agentbox-runtime.1`, and `share/man/man1/agentbox-completion.1`, or matching `.gz` manual pages when the package output compresses manual pages

`nix build '.#default'` must produce those files in its result path.
