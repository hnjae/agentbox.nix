#!/bin/sh
set -eu

fixtures=${AGENTBOX_TEST_FIXTURES:?missing AGENTBOX_TEST_FIXTURES}
log_path=${AGENTBOX_TEST_LOG:-}

record() {
    if [ "$log_path" != "" ]; then
        op=$1
        shift
        printf '%s lock=%s args=%s\n' "$op" "$(lock_state)" "$*" >>"$log_path"
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

maybe_fail() {
    prefix=$1
    if [ -f "$fixtures/$prefix.exit" ]; then
        if [ -f "$fixtures/$prefix.stderr" ]; then
            cat "$fixtures/$prefix.stderr" >&2
        fi
        exit "$(tr -d '\n' <"$fixtures/$prefix.exit")"
    fi
}

has_flag() {
    flag=$1
    shift
    for arg in "$@"; do
        if [ "$arg" = "$flag" ]; then
            return 0
        fi
    done
    return 1
}

last_arg() {
    last=
    for arg in "$@"; do
        last=$arg
    done
    printf '%s\n' "$last"
}

safe_image_name() {
    printf '%s' "$1" | tr -c 'A-Za-z0-9_.-' '_'
}

validate_build_context() {
    containerfile=
    context_dir=

    while [ "$#" -gt 0 ]; do
        case "$1" in
        -f)
            shift
            containerfile=${1:-}
            ;;
        esac

        context_dir=$1
        shift || true
    done

    [ "$containerfile" != "" ] || {
        printf 'missing build containerfile argument\n' >&2
        exit 98
    }

    [ "$context_dir" != "" ] || {
        printf 'missing build context directory argument\n' >&2
        exit 98
    }

    [ -r "$containerfile" ] || {
        printf 'unreadable build containerfile: %s\n' "$containerfile" >&2
        exit 98
    }

    [ "$containerfile" = "$context_dir/Containerfile" ] || {
        printf 'build containerfile %s did not match context %s\n' "$containerfile" "$context_dir" >&2
        exit 98
    }

    for relative_path in \
        Containerfile \
        bootstrap \
        entrypoint \
        lib/runtime-contract.sh \
        runtime-packages.nix; do
        [ -r "$context_dir/$relative_path" ] || {
            printf 'missing embedded build file: %s\n' "$relative_path" >&2
            exit 98
        }
    done
}

cmd=$1
shift || true

case "$cmd" in
ps)
    record ps "$@"
    cat "$fixtures/ps.json"
    ;;
image)
    record image "$@"
    subcommand=${1:-}
    shift || true
    case "$subcommand" in
    exists)
        target=${1:?missing image exists target}
        if [ -f "$fixtures/image.exists" ] || [ -f "$fixtures/image-exists-$(safe_image_name "$target")" ]; then
            exit 0
        fi
        exit 1
        ;;
    ls)
        cat "$fixtures/images.json"
        ;;
    rm)
        maybe_fail image-rm
        printf 'removed image\n'
        ;;
    *)
        printf 'unexpected podman image invocation: %s %s\n' "$subcommand" "$*" >&2
        exit 97
        ;;
    esac
    ;;
volume)
    record volume "$@"
    subcommand=${1:-}
    shift || true
    case "$subcommand" in
    ls)
        cat "$fixtures/volumes.json"
        ;;
    exists)
        target=${1:?missing volume exists target}
        if [ -f "$fixtures/volume-exists-$target" ]; then
            exit 0
        fi
        exit 1
        ;;
    rm)
        maybe_fail volume-rm
        printf 'removed volume\n'
        ;;
    *)
        printf 'unexpected podman volume invocation: %s %s\n' "$subcommand" "$*" >&2
        exit 97
        ;;
    esac
    ;;
container)
    subcommand=${1:-}
    shift || true
    case "$subcommand" in
    exists)
        target=${1:?missing container exists target}
        record container-exists "$@"
        if [ -f "$fixtures/container-exists-$target" ]; then
            exit 0
        fi
        exit 1
        ;;
    *)
        printf 'unexpected podman container invocation: %s %s\n' "$subcommand" "$*" >&2
        exit 97
        ;;
    esac
    ;;
build)
    validate_build_context "$@"
    record build "$@"
    maybe_fail build
    printf 'built\n'
    ;;
inspect)
    target=${1:?missing inspect target}
    record inspect "$@"
    fixture="$fixtures/inspect-$target.json"
    if [ ! -f "$fixture" ]; then
        printf 'no such object: %s\n' "$target" >&2
        exit 125
    fi
    cat "$fixture"
    ;;
run)
    record run "$@"
    maybe_fail run
    printf 'started\n'
    ;;
logs)
    record logs "$@"
    target="$(last_arg "$@")"
    fixture="$fixtures/logs-$target.txt"
    if [ ! -f "$fixture" ]; then
        printf 'no logs for %s\n' "$target" >&2
        exit 125
    fi
    cat "$fixture"
    ;;
stop)
    record stop "$@"
    if [ -f "$fixtures/missing-during-cleanup" ] && ! has_flag --ignore "$@"; then
        printf 'no such object: %s\n' "$(last_arg "$@")" >&2
        exit 125
    fi
    maybe_fail stop
    printf 'stopped\n'
    ;;
rm)
    record rm "$@"
    if [ -f "$fixtures/missing-during-cleanup" ] && ! has_flag --ignore "$@"; then
        printf 'no such object: %s\n' "$(last_arg "$@")" >&2
        exit 125
    fi
    maybe_fail rm
    printf 'removed\n'
    ;;
*)
    printf 'unexpected podman invocation: %s %s\n' "$cmd" "$*" >&2
    exit 97
    ;;
esac
