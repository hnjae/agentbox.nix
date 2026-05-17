#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

. "$(dirname "$0")/_runtime-contract-helpers.sh"

repo_root=$(runtime_contract_repo_root "$0")

mode=${1-}

usage() {
    printf 'Usage: %s --mode {xdg-state-fallback|fixed-home-fallback}\n' "$0" >&2
    exit 1
}

if [ "$mode" != "--mode" ] || [ "${2-}" = "" ] || [ "${3-}" != "" ]; then
    usage
fi

mode=$2

case "$mode" in
xdg-state-fallback | fixed-home-fallback) ;;
*) usage ;;
esac

image_tag=containerfile-nixkpgs-runtime-compat-$mode
container_name=containerfile-nixkpgs-runtime-compat-$mode-$$
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

case "$mode" in
xdg-state-fallback)
    podman exec "$container_name" env -u XDG_STATE_HOME /entrypoint zsh -ceu '
            test "$NIX_PROFILE_PATH" = "$HOME/.local/state/nix/profile"
            case "$PATH" in
                *"$HOME/.local/state/nix/profile/bin"*) ;;
                *) exit 1 ;;
            esac

            command -v gh >/dev/null 2>&1
        '
    ;;
fixed-home-fallback)
    podman exec "$container_name" env -u XDG_STATE_HOME -u HOME /entrypoint zsh -ceu '
            test "$HOME" = "/home/user"
            test "$NIX_PROFILE_PATH" = "/home/user/.local/state/nix/profile"
            case "$NIX_PROFILES" in
                *"/home/user/.local/state/nix/profile"*) ;;
                *) exit 1 ;;
            esac

            case ":$PATH:" in
                *:"/home/user/.local/state/nix/profile/bin":*) ;;
                *) exit 1 ;;
            esac

            command -v gh >/dev/null 2>&1
        '
    ;;
esac

printf 'podman runtime compatibility OK (%s)\n' "$mode"
