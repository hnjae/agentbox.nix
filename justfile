#!/usr/bin/env -S just --justfile

set unstable
set lazy
set fallback := false

_:
    @just --list

[group('ci')]
format:
    devenv shell -- treefmt

[group('ci')]
lint-fix:
    cargo clippy --fix --allow-dirty
    deadnix --edit

[group('ci')]
lint:
    devenv tasks run --mode single ci:lint --no-tui

[group('ci')]
test:
    devenv tasks run --mode single ci:test --no-tui

[group('ci')]
check:
    devenv tasks run ci

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

[group('nix')]
flake-show:
    nix --no-warn-dirty flake show

[group('nix')]
flake-update:
    nix flake update
    devenv update

[group('tools')]
clean:
    rm -rf result target
