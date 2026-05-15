#!/usr/bin/env -S just --justfile

set unstable
set lazy
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
    find . -type f \( -name '*.sh' -o -name '*.bash' -o -name '.envrc' -o -name '.envrc.*' -o -name '.env' -o -name '.env.*' \) -exec shellcheck -e SC2034,SC1091,SC2154 {} +
    shellharden --check

    # nix
    statix check
    deadnix --fail

    # Markdown
    rumdl check --exclude .sisyphus/plans/agentbox-mvp.md

    # Misc
    typos
    # Repo-wide editorconfig debt remains in sacred/generated files.

[group('ci')]
test:
    cargo test

[group('ci')]
check: static-checks test

[group('build')]
build:
    nix --no-warn-dirty build -- '.#agentbox'

[group('build')]
[script('sh')]
install:
    set -eu

    repo_url="git+file://$(git rev-parse --show-toplevel)"
    profile_json="$(nix profile list --json)"

    if printf '%s\n' "${profile_json}" \
        | jq --exit-status --arg repo_url "${repo_url}" \
            '.elements.agentbox.originalUrl == $repo_url' \
            >/dev/null
    then
        nix --no-warn-dirty profile upgrade agentbox
    else
        nix --no-warn-dirty profile add -- '.#agentbox'
    fi

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
