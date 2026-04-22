#!/bin/sh

set -eu

. "$(dirname "$0")/_runtime-contract-helpers.sh"

script_dir=$( 
    CDPATH=
    export CDPATH
    cd -- "$(dirname "$0")"
    pwd
)
repo_root=$( 
    CDPATH=
    export CDPATH
    cd -- "$script_dir/.."
    pwd
)

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
    xdg-state-fallback|fixed-home-fallback) ;;
    *) usage ;;
esac

image_tag=containerfile-nixkpgs-runtime-compat-$mode
container_name=containerfile-nixkpgs-runtime-compat-$mode-$$
container_home=/home/user

cleanup() {
    runtime_contract_cleanup
}

wait_for_startup() {
    attempts=0

    while :; do
        if podman exec "$container_name" /bin/sh -ceu '
            test -s /tmp/startup-path
            test -s /tmp/startup-profiles
            test -s /tmp/startup-xdg-data-dirs
        ' >/dev/null 2>&1; then
            return 0
        fi

        attempts=$((attempts + 1))
        if [ "$attempts" -ge 300 ]; then
            printf 'Timed out waiting for startup contract artifacts\n' >&2
            podman logs "$container_name" >&2 || true
            exit 1
        fi

        sleep 1
    done
}

trap cleanup EXIT INT TERM HUP

runtime_contract_require_command podman
runtime_contract_require_command id

runtime_contract_prepare_cache_volume_mount
runtime_contract_check_host_paths
nix_client_path=$(runtime_contract_resolve_nix_client)

set -- \
    --userns=keep-id:size=65536 \
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
    --mount "$home_mount" \
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
wait_for_startup

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
