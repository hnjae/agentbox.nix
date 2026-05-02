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

Follow spec-driven development for product behavior and user-facing implementation
changes. Before implementing a product behavior change, update `SPEC.md` to
describe the intended behavior from the user's perspective, commit that
specification update, and then carry out the work according to the committed spec.
Do not update `SPEC.md` for implementation-dependent changes that do not affect
user-visible behavior. This includes development-only or verification-only
changes, such as build tooling, formatter/linter/test configuration, CI wiring,
dependency metadata, or repository maintenance.

## Commit Guidelines

Use Conventional Commits style for commit messages, such as `feat: add nix package output` or `fix: handle missing cargo metadata`.
