#!/bin/sh

set -eu

. "$(dirname "$0")/_runtime-contract-helpers.sh"

repo_root=$(runtime_contract_repo_root "$0")

image_tag=containerfile-nixkpgs-entrypoint-contract
container_name=containerfile-nixkpgs-entrypoint-contract-$$
container_home=/home/user
profile_path=$container_home/.local/state/nix/profile

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

podman exec "$container_name" /bin/sh -ceu "
    test \"\$(id -u)\" = 1000
    test \"\$(id -g)\" = '$runtime_contract_host_gid'
    test -w '$container_home'
    test -w '$container_home/.cache'
    test -d '$container_home/.cache/nix'
    test -w '$container_home/.cache/nix'
    test -w '$container_home/.config'
    test -w '$container_home/.local'
    test -w '$container_home/.local/state'
    test -x '$profile_path/bin/gh'
    touch '$container_home/.cache/nix/agentbox-writable'
    touch '$container_home/.local/state/agentbox-writable'
    read -r startup_path </tmp/startup-path
    read -r startup_profiles </tmp/startup-profiles
    read -r startup_xdg_data_dirs </tmp/startup-xdg-data-dirs

    case \"\$startup_path\" in
        *'$profile_path/bin'*) ;;
        *) exit 1 ;;
    esac

    case \"\$startup_profiles\" in
        *'$profile_path'*) ;;
        *) exit 1 ;;
    esac

    case \"\$startup_xdg_data_dirs\" in
        *'$profile_path/share'*) ;;
        *) exit 1 ;;
    esac
"

podman exec "$container_name" /entrypoint /bin/sh -ceu "
    test -r \"\$NIX_SSL_CERT_FILE\"

    case \"\$NIX_PROFILES\" in
        *'$profile_path'*) ;;
        *) exit 1 ;;
    esac

    case \":\$PATH:\" in
        *:'$profile_path/bin':*) ;;
        *) exit 1 ;;
    esac

    case \"\$XDG_DATA_DIRS\" in
        *'$profile_path/share'*) ;;
        *) exit 1 ;;
    esac

    command -v gh >/dev/null 2>&1
"

podman exec "$container_name" env -u USER -u LOGNAME -u HOME /entrypoint /bin/sh -ceu "
    test -r \"\$NIX_SSL_CERT_FILE\"

    case \"\$NIX_PROFILES\" in
        *'$profile_path'*) ;;
        *) exit 1 ;;
    esac

    case \":\$PATH:\" in
        *:'$profile_path/bin':*) ;;
        *) exit 1 ;;
    esac

    case \"\$XDG_DATA_DIRS\" in
        *'$profile_path/share'*) ;;
        *) exit 1 ;;
    esac

    command -v gh >/dev/null 2>&1
"

printf 'podman entrypoint contract OK\n'
