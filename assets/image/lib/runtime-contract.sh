#!/bin/sh

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

_resolve_path() {
    path=$1

    readlink -f "$path" 2>/dev/null || printf '%s' "$path"
}

_script_dir() {
    script_path=$(_resolve_path "$0")

    case "$script_path" in
    */*)
        printf '%s\n' "${script_path%/*}"
        ;;
    *)
        printf '.\n'
        ;;
    esac
}

check_cond() {
    message=$1
    shift

    if ! "$@"; then
        die "$message"
    fi
}

_is_readable_target() {
    path=$1

    [ -r "$path" ] || return 1

    resolved=$(_resolve_path "$path")
    [ -r "$resolved" ]
}

_is_empty_dir() {
    path=$1

    [ -d "$path" ] || return 1

    set -- "$path"/.[!.]* "$path"/..?* "$path"/*
    [ ! -e "$1" ]
}

_is_mountpoint() {
    path=$1

    [ -r /proc/self/mountinfo ] || return 1

    while IFS=' ' read -r _ _ _ _ mount_point _; do
        [ "$mount_point" = "$path" ] && return 0
    done </proc/self/mountinfo

    return 1
}

resolve_ca_bundle_path() {
    if [ -r /etc/ssl/certs/ca-certificates.crt ]; then
        printf '%s\n' /etc/ssl/certs/ca-certificates.crt
    else
        return 1
    fi
}

_has_nix_command() {
    command -v nix >/dev/null 2>&1
}

validate_runtime_transport() {
    check_cond "Missing host nix-daemon socket at: ${NIX_DAEMON_SOCKET_PATH}. Mount /nix:/nix:ro." test -S "$NIX_DAEMON_SOCKET_PATH"
    check_cond "Expected host-mounted nix not found in PATH. Mount /run/current-system/sw/bin/nix:/usr/local/bin/nix:ro or /nix/var/nix/profiles/default/bin/nix:/usr/local/bin/nix:ro." _has_nix_command
}

validate_runtime_config() {
    check_cond "Missing /etc/nix host mount. Mount /etc/nix:/etc/nix:ro so the wrapper inherits the host config and registry." _is_mountpoint /etc/nix
    check_cond "Missing readable host Nix config: /etc/nix/nix.conf. Mount /etc/nix:/etc/nix:ro." test -r /etc/nix/nix.conf

    if [ -e /etc/nix/nix.custom.conf ] || [ -L /etc/nix/nix.custom.conf ]; then
        check_cond "Missing readable target for /etc/nix/nix.custom.conf. Mount /etc/static/nix:/etc/static/nix:ro when /etc/nix points there." _is_readable_target /etc/nix/nix.custom.conf
    fi
}

validate_runtime_trust() {
    check_cond "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt." resolve_ca_bundle_path
}

resolve_runtime_paths() {
    runtime_home=${HOME:-/home/user}

    HOME=$runtime_home

    if [ "${XDG_CACHE_HOME-}" = "" ]; then
        XDG_CACHE_HOME="$runtime_home/.cache"
    fi

    if [ "${XDG_STATE_HOME-}" = "" ]; then
        XDG_STATE_HOME="$runtime_home/.local/state"
    fi

    NIX_PROFILE_PATH="$XDG_STATE_HOME/nix/profile"

    export HOME XDG_CACHE_HOME XDG_STATE_HOME NIX_PROFILE_PATH
}

resolve_runtime_manifest_path() {
    NIX_RUNTIME_PACKAGES_FILE="$(_script_dir)/runtime-packages.nix"
    export NIX_RUNTIME_PACKAGES_FILE
}

activate_ca_bundle_env() {
    NIX_SSL_CERT_FILE=$(resolve_ca_bundle_path) ||
        die "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt."

    export NIX_SSL_CERT_FILE
}

materialize_profile() {
    if ! mkdir -p "$XDG_STATE_HOME/nix" >/dev/null 2>&1; then
        die "Unusable Nix profile state path: $NIX_PROFILE_PATH. Ensure XDG_STATE_HOME or HOME points to a writable location."
    fi

    if nix --extra-experimental-features "nix-command flakes" profile list --profile "$NIX_PROFILE_PATH" >/dev/null 2>&1 &&
        [ -x "$NIX_PROFILE_PATH/bin/gh" ]; then
        return 0
    fi

    if _is_empty_dir "$NIX_PROFILE_PATH"; then
        rmdir "$NIX_PROFILE_PATH"
    elif [ -e "$NIX_PROFILE_PATH" ]; then
        die "Existing Nix profile is not usable at: $NIX_PROFILE_PATH. Remove it or choose a different XDG_STATE_HOME."
    fi

    check_cond "Missing runtime package manifest: ${NIX_RUNTIME_PACKAGES_FILE}." test -r "$NIX_RUNTIME_PACKAGES_FILE"

    # Validate the startup path by exercising nix profile add against the
    # host-attached client and inherited host config via the package manifest.
    nix --extra-experimental-features "nix-command flakes" profile add \
        --profile "$NIX_PROFILE_PATH" \
        --file "$NIX_RUNTIME_PACKAGES_FILE" \
        runtime
}

activate_profile_env() {
    default_profile_path=/nix/var/nix/profiles/default
    profile_link=$NIX_PROFILE_PATH

    export NIX_PROFILES="$default_profile_path $profile_link"

    # Populate bash completions, .desktop files, etc
    if [ "${XDG_DATA_DIRS-}" = "" ]; then
        # According to XDG spec the default is /usr/local/share:/usr/share, don't set something that prevents that default
        export XDG_DATA_DIRS="/usr/local/share:/usr/share:$profile_link/share:$default_profile_path/share"
    else
        export XDG_DATA_DIRS="$XDG_DATA_DIRS:$profile_link/share:$default_profile_path/share"
    fi

    export PATH="$profile_link/bin:$default_profile_path/bin${PATH:+:$PATH}"
    unset default_profile_path profile_link
}

activate_runtime_base_env() {
    validate_runtime_transport
    validate_runtime_config
    validate_runtime_trust
    resolve_runtime_paths
    activate_ca_bundle_env
}

runtime_entrypoint_main() {
    activate_runtime_base_env
    activate_profile_env

    exec "$@"
}

runtime_bootstrap_main() {
    activate_runtime_base_env
    resolve_runtime_manifest_path
    materialize_profile

    exec /entrypoint "$@"
}
