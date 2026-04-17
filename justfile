#!/usr/bin/env -S just --justfile

set unstable := true
set lazy := true
set fallback := false

_:
    @just --list

[group('ci')]
format:
    nix --no-warn-dirty fmt

[group('ci')]
lint-fix:
    cargo clippy --fix --allow-dirty
    deadnix --edit

alias lint := static-checks

[group('ci')]
static-checks:
    # Rust
    cargo fmt --check
    cargo clippy --all-targets --all-features

    # Shellfiles
    fd --hidden --type f '(\.(sh|bash)|\.env(rc)?(\..+)?)$' --exec-batch shellcheck {}
    shellharden --check

    # nix
    statix check
    deadnix --fail

    # Markdown
    rumdl check

    # Misc
    typos
    editorconfig-checker

[group('ci')]
test:
    cargo test

[group('ci')]
check: static-checks test

[group('build')]
build:
    nix --no-warn-dirty build -- '.#default'

[group('build')]
build-rs:
    cargo build --quiet

[group('ci')]
flake-check:
    nix --no-warn-dirty flake check

[group('nix')]
flake-show:
    nix --no-warn-dirty flake show

[group('nix')]
flake-update:
    nix flake update
    nix flake update --flake ./nix/partitions/dev

[group('tools')]
clean:
    rm -rf result target
