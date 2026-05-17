#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

fixtures=${AGENTBOX_TEST_FIXTURES:?missing AGENTBOX_TEST_FIXTURES}
log_path=${AGENTBOX_TEST_LOG:-}

record() {
    if [ "$log_path" != "" ]; then
        printf 'nix lock=%s args=%s\n' "$(lock_state)" "$*" >>"$log_path"
    fi
}

lock_state() {
    lock_path=${AGENTBOX_TEST_LOCK_PATH:-}
    lock_probe=${AGENTBOX_TEST_LOCK_PROBE:-}
    if [ "$lock_path" != "" ] && [ "$lock_probe" != "" ]; then
        "$lock_probe" "$lock_path"
    else
        printf 'unknown'
    fi
}

safe_name() {
    printf '%s' "$1" | tr -c 'A-Za-z0-9_.-' '_'
}

expr_arg() {
    expression=
    while [ "$#" -gt 0 ]; do
        case "$1" in
        --expr)
            shift
            expression=${1:-}
            ;;
        esac
        shift || true
    done
    printf '%s\n' "$expression"
}

cmd=${1:-}
shift || true

case "$cmd" in
eval)
    record eval "$@"
    expr=$(expr_arg "$@")
    flake_ref=$(printf '%s\n' "$expr" | sed -n 's/.*builtins.getFlake "\([^"]*\)".*/\1/p')
    attr=$(printf '%s\n' "$expr" | sed -n 's/.*builtins.hasAttr "\([^"]*\)".*/\1/p')
    flake_root=${flake_ref#path:}
    safe_root=$(safe_name "$flake_root")
    safe_attr=$(safe_name "$attr")

    failure="$fixtures/nix-eval-fail-$safe_root.stderr"
    if [ -f "$failure" ]; then
        cat "$failure" >&2
        exit 1
    fi

    if [ -f "$fixtures/devshell-$safe_root-$safe_attr" ]; then
        printf 'true\n'
    else
        printf 'false\n'
    fi
    ;;
*)
    printf 'unexpected nix invocation: %s %s\n' "$cmd" "$*" >&2
    exit 97
    ;;
esac
