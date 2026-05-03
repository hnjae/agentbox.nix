# Repository Guidelines

## Build, Test, and Development Commands

- `cargo build`: compile the Rust crate with Cargo.
- `cargo test`: run the Rust test suite.
- `just static-checks`: run formatting checks and linters.
- `nix flake check`: validate the Nix flake.
- `nix build '.#default'`: build the default Nix package.
- `nix fmt`: apply repository formatting.

## Coding Style & Naming Conventions

Follow standard Rust formatting with `cargo fmt`; Use 4-space indentation.

## Spec-Driven Development

For user-visible behavior changes, update `SPEC.md` before changing implementation code. The spec update must describe the intended behavior from the user's perspective and should be committed first unless the user explicitly asks not to commit or asks to pause.

Do not update `SPEC.md` for changes that do not affect user-visible behavior, including build tooling, formatter/linter/test configuration, CI wiring, dependency metadata, and repository maintenance.

## Commit Guidelines

Use Conventional Commits style for commit messages, such as `feat: add nix package output`.

When you modify tracked project files, create a commit before ending the task unless the user explicitly asks not to commit or asks to pause. If the work naturally splits into independent steps, use separate commits and keep the `SPEC.md` update as the first commit for behavior changes.

Do not include unrelated user changes in your commits.
