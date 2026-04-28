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

## Commit Guidelines

Use Conventional Commits style for commit messages, such as `feat: add nix package output` or `fix: handle missing cargo metadata`.
