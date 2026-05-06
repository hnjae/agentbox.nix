#!/bin/sh
# shellcheck disable=SC2154

# Runtime contract matrix (frozen from README + current test behavior):
# - Required host mounts: /nix, host nix in PATH, /etc/nix
# - Optional host mounts: /etc/static/nix
# - Required ephemeral writable home mount: tmpfs at /home/user
# - Required Podman-managed mount: anonymous volume at /home/user/.cache/nix
# - Supported later-exec path: /entrypoint <cmd>
# - Startup/bootstrap contract remains owned by the caller test script

runtime_contract_require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'Missing required command: %s\n' "$1" >&2
        exit 1
    fi
}

runtime_contract_check_host_paths() {
    if [ ! -d /nix ]; then
        printf 'Missing required host path: /nix\n' >&2
        exit 1
    fi

    if [ ! -d /etc/nix ]; then
        printf 'Missing required host path: /etc/nix\n' >&2
        exit 1
    fi
}

runtime_contract_repo_root() {
    runtime_contract_script_dir=$(
        CDPATH=
        export CDPATH
        cd -- "$(dirname "$1")" || return 1
        pwd
    )

    CDPATH=
    export CDPATH
    cd -- "$runtime_contract_script_dir/.." || return 1
    pwd
}

runtime_contract_resolve_nix_client() {
    if [ -x /run/current-system/sw/bin/nix ]; then
        printf '%s\n' /run/current-system/sw/bin/nix
    elif [ -x /nix/var/nix/profiles/default/bin/nix ]; then
        printf '%s\n' /nix/var/nix/profiles/default/bin/nix
    else
        printf 'Could not find a host nix client to mount\n' >&2
        exit 1
    fi
}

runtime_contract_prepare_cache_volume_mount() {
    home_mount="type=tmpfs,dst=/home/user,tmpfs-mode=1777"
    cache_mount="type=volume,dst=/home/user/.cache/nix,U"
}

runtime_contract_prepare_user_args() {
    runtime_contract_host_gid=$(id -g)
    runtime_contract_userns="keep-id:uid=1000,gid=$runtime_contract_host_gid"
    runtime_contract_user="user:$runtime_contract_host_gid"
}

runtime_contract_build_image() {
    # shellcheck disable=SC2154
    podman build -t "$image_tag" -f "$repo_root/Containerfile" "$repo_root"
}

runtime_contract_run_container() {
    # shellcheck disable=SC2154
    podman run -d "$@" "$image_tag" /bin/sh -ceu '
        printf "%s\n" "$PATH" >/tmp/startup-path
        printf "%s\n" "$NIX_PROFILES" >/tmp/startup-profiles
        printf "%s\n" "$XDG_DATA_DIRS" >/tmp/startup-xdg-data-dirs
        sleep infinity
    '
}

runtime_contract_wait_for_startup() {
    runtime_contract_container_name=$1
    attempts=0

    while :; do
        if podman exec "$runtime_contract_container_name" /bin/sh -ceu '
            test -s /tmp/startup-path
            test -s /tmp/startup-profiles
            test -s /tmp/startup-xdg-data-dirs
        ' >/dev/null 2>&1; then
            return 0
        fi

        attempts=$((attempts + 1))
        if [ "$attempts" -ge 300 ]; then
            printf 'Timed out waiting for startup contract artifacts\n' >&2
            podman logs "$runtime_contract_container_name" >&2 || true
            exit 1
        fi

        sleep 1
    done
}

runtime_contract_cleanup() {
    # shellcheck disable=SC2154
    if [ "${container_name-}" != "" ]; then
        podman rm -f --volumes "$container_name" >/dev/null 2>&1 || true
    fi
}
