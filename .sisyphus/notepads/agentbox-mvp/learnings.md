# Learnings

- `cargo build` refreshed `Cargo.lock` with the new registry entries, and `cargo test --locked` succeeds once the lockfile is updated.
- Clap integration tests can verify the public surface two ways at once: `assert_cmd` covers binary help/error exit behavior, while `Cli::try_parse_from(...)` gives direct assertions for each parsed subcommand without needing any runtime implementation.
- The spec’s worked naming example for `/aaa/bbb` needs an exact `hash12` expectation in tests; the identity code should keep the readable suffix deterministic and preserve the rightmost characters when truncating overlong names.
- The `/aaa/bbb` example text did not match the literal SHA-256 digest; the code now treats the algorithm as authoritative and the tests pin the real derived digest/name.
2026-04-21: `directories::BaseDirs::new()` is the simplest reusable way to honor `XDG_STATE_HOME` for lock placement, and `fd-lock` can wrap an existing lock file without treating stale unlocked `.lock` files as metadata.
