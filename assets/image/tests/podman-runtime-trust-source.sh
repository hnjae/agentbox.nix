#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

. "$(dirname "$0")/_runtime-contract-helpers.sh"

repo_root=$(runtime_contract_repo_root "$0")

mode=${1-}

usage() {
    printf 'Usage: %s --mode {image-local}\n' "$0" >&2
    exit 1
}

if [ "$mode" != "--mode" ] || [ "${2-}" = "" ] || [ "${3-}" != "" ]; then
    usage
fi

mode=$2

case "$mode" in
image-local) ;;
*) usage ;;
esac

image_tag=containerfile-nixkpgs-runtime-trust-source-$mode
container_name=containerfile-nixkpgs-runtime-trust-source-$mode-$$
container_home=/home/user

cleanup() {
    runtime_contract_cleanup
}

trap cleanup EXIT INT TERM HUP

runtime_contract_require_command podman
runtime_contract_require_command id

runtime_contract_prepare_user_args
runtime_contract_prepare_cache_volume_mount
runtime_contract_check_host_paths
nix_client_path=$(runtime_contract_resolve_nix_client)

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
    -v /etc/nix:/etc/nix:ro \
    -v "$nix_client_path:/usr/local/bin/nix:ro"

if [ -e /etc/static/nix ]; then
    set -- "$@" -v /etc/static/nix:/etc/static/nix:ro
fi

runtime_contract_build_image
runtime_contract_run_container "$@"

runtime_contract_wait_for_startup "$container_name"

startup_output=$(podman logs "$container_name" 2>&1)

case "$startup_output" in
*'/etc/ssl/certs/ca-certificates.crt'*)
    printf 'Runtime startup leaked CA bundle probe output:\n%s\n' "$startup_output" >&2
    exit 1
    ;;
esac

if ! entrypoint_output=$(podman exec "$container_name" /entrypoint /bin/sh -ceu '
    test -r "$NIX_SSL_CERT_FILE"
    test "$NIX_SSL_CERT_FILE" = /etc/ssl/certs/ca-certificates.crt
' 2>&1); then
    printf '%s\n' "$entrypoint_output" >&2
    exit 1
fi

case "$entrypoint_output" in
*'/etc/ssl/certs/ca-certificates.crt'*)
    printf 'Runtime entrypoint leaked CA bundle probe output:\n%s\n' "$entrypoint_output" >&2
    exit 1
    ;;
?*)
    printf 'Runtime entrypoint wrote unexpected output:\n%s\n' "$entrypoint_output" >&2
    exit 1
    ;;
esac

printf 'podman runtime trust source OK (%s)\n' "$mode"
