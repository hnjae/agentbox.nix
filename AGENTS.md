# Repository Guidelines

## Documentation

- Keep user-facing product and behavior specifications in `SPEC.md`, not internal implementation details.
- Write all documentation in a concise, technical style.
- Do not hard-wrap prose in Markdown files. Keep ordinary paragraphs and list items on a single source line unless Markdown syntax or structured blocks require line breaks.
- Use Mermaid for diagrams when diagrams are needed.

## Build, Test, and Development Commands

- `cargo build`: compile the Rust crate with Cargo.
- `cargo test`: run the Rust test suite.
- `just static-checks`: run formatting checks and linters.
- `nix flake check`: validate the Nix flake.
- `nix build '.#default'`: build the default Nix package.
- `nix fmt`: apply repository formatting.

## Compatibility Policy

agentbox.nix is still pre-release. Do not spend effort preserving backward compatibility or writing migrations for existing user data, configuration, APIs, or internal formats unless explicitly requested.

## Coding Style & Naming Conventions

Follow standard Rust formatting with `cargo fmt`; Use 4-space indentation.

## Spec-Driven Development

For user-visible behavior changes, update `SPEC.md` before changing implementation code. The spec update must describe the intended behavior from the user's perspective and should be committed first unless the user explicitly asks not to commit or asks to pause.

Do not update `SPEC.md` for changes that do not affect user-visible behavior, including build tooling, formatter/linter/test configuration, CI wiring, dependency metadata, and repository maintenance.

## Commit Guidelines

- Use Conventional Commits for all commit messages.
- Use a Conventional Commit scope when a clear scope exists.
- When a task is complete and the user has not said otherwise, commit the changes.
- Do not include unrelated user changes in your commits.
