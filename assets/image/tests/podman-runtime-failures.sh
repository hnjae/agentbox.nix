#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

. "$(dirname "$0")/_runtime-contract-helpers.sh"

repo_root=$(runtime_contract_repo_root "$0")

mode=${1-}

usage() {
    printf 'Usage: %s --mode {missing-etc-nix|broken-static-nix|no-ca-bundle|unusable-state-path}\n' "$0" >&2
    exit 1
}

if [ "$mode" != "--mode" ] || [ "${2-}" = "" ] || [ "${3-}" != "" ]; then
    usage
fi

mode=$2

case "$mode" in
missing-etc-nix | broken-static-nix | no-ca-bundle | unusable-state-path) ;;
*) usage ;;
esac

image_tag=containerfile-nixkpgs-runtime-failure-contract
container_name=containerfile-nixkpgs-runtime-failure-$mode-$$
container_home=/home/user
evidence_dir=$repo_root/.sisyphus/evidence
evidence_file=$evidence_dir/podman-runtime-failures-$mode.log
custom_etc_nix=
empty_ssl_dir=

cleanup() {
    runtime_contract_cleanup
    rm -rf "$custom_etc_nix" "$empty_ssl_dir"
}

run_failure_command() {
    logfile=$1
    shift

    rm -f "$logfile"

    set +e
    "$@" >/dev/null 2>"$logfile"
    status=$?
    set -e

    if [ "$status" -eq 0 ]; then
        printf 'Expected runtime failure, but command succeeded for mode: %s\n' "$mode" >&2
        if [ -r "$logfile" ]; then
            cat "$logfile" >&2
        fi
        exit 1
    fi
}

assert_log_contains() {
    expected=$1

    if ! grep -F -- "$expected" "$evidence_file" >/dev/null 2>&1; then
        printf 'Missing expected diagnostic for mode %s\n' "$mode" >&2
        printf 'Expected substring: %s\n' "$expected" >&2
        printf 'Captured stderr (%s):\n' "$evidence_file" >&2
        cat "$evidence_file" >&2
        exit 1
    fi
}

trap cleanup EXIT INT TERM HUP

runtime_contract_require_command podman
runtime_contract_require_command id
runtime_contract_check_host_paths
runtime_contract_prepare_user_args
runtime_contract_prepare_cache_volume_mount
nix_client_path=$(runtime_contract_resolve_nix_client)

mkdir -p "$evidence_dir"

runtime_contract_build_image

set -- \
    --userns "$runtime_contract_userns" \
    --user "$runtime_contract_user" \
    --group-add keep-groups \
    --workdir /workspace \
    --name "$container_name" \
    -e USER="$(id -un)" \
    -e LOGNAME="$(id -un)" \
    -e HOME="$container_home" \
    -e XDG_BIN_HOME="$container_home"/.local/bin \
    -e XDG_CACHE_HOME="$container_home"/.cache \
    -e XDG_STATE_HOME="$container_home"/.local/state \
    -e XDG_CONFIG_HOME="$container_home"/.config \
    -e XDG_DATA_HOME="$container_home"/.local/share \
    -e ZDOTDIR="$container_home"/.config/zsh \
    --mount "$cache_mount" \
    -v "$repo_root:/workspace" \
    -v /nix:/nix:ro \
    -v "$nix_client_path:/usr/local/bin/nix:ro"

case "$mode" in
missing-etc-nix)
    if [ -e /etc/static/nix ]; then
        set -- "$@" -v /etc/static/nix:/etc/static/nix:ro
    fi

    run_failure_command "$evidence_file" podman run --rm "$@" "$image_tag" /bin/true
    assert_log_contains "Missing /etc/nix host mount. Mount /etc/nix:/etc/nix:ro so the wrapper inherits the host config and registry."
    ;;
broken-static-nix)
    custom_etc_nix=$(mktemp -d /tmp/containerfile-nixpkgs-runtime-failure-etc-nix.XXXXXX)
    printf 'experimental-features = nix-command flakes\n' >"$custom_etc_nix/nix.conf"
    ln -s /etc/static/nix/nix.custom.conf "$custom_etc_nix/nix.custom.conf"

    set -- "$@" -v "$custom_etc_nix:/etc/nix:ro"

    run_failure_command "$evidence_file" podman run --rm "$@" "$image_tag" /bin/true
    assert_log_contains "Missing readable target for /etc/nix/nix.custom.conf. Mount /etc/static/nix:/etc/static/nix:ro when /etc/nix points there."
    ;;
no-ca-bundle)
    empty_ssl_dir=$(mktemp -d /tmp/containerfile-nixpkgs-runtime-failure-ssl.XXXXXX)

    set -- "$@" \
        -v /etc/nix:/etc/nix:ro \
        -v "$empty_ssl_dir:/etc/ssl:ro"

    if [ -e /etc/static/nix ]; then
        set -- "$@" -v /etc/static/nix:/etc/static/nix:ro
    fi

    run_failure_command "$evidence_file" podman run --rm "$@" "$image_tag" /bin/true
    assert_log_contains "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt."
    ;;
unusable-state-path)
    set -- "$@" -v /etc/nix:/etc/nix:ro

    if [ -e /etc/static/nix ]; then
        set -- "$@" -v /etc/static/nix:/etc/static/nix:ro
    fi

    run_failure_command "$evidence_file" podman run --rm "$@" -e XDG_STATE_HOME=/proc/agentbox-state "$image_tag" /bin/true
    assert_log_contains "Unusable Nix profile state path: /proc/agentbox-state/nix/profile. Ensure XDG_STATE_HOME or HOME points to a writable location."
    ;;
esac

printf 'podman runtime failure contract OK (%s)\n' "$mode"
