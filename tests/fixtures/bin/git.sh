#!/bin/sh
set -eu

safe_git_config_key() {
    printf '%s' "$1" | tr -c 'A-Za-z0-9_-' '_'
}

if [ "$1" = "-C" ] && [ "$3" = "rev-parse" ] && [ "$4" = "--show-toplevel" ]; then
    dir=$2
    while [ "$dir" != "/" ]; do
        if [ -d "$dir/.git" ]; then
            printf '%s\n' "$dir"
            exit 0
        fi
        dir=$(dirname "$dir")
    done

    printf 'fatal: not a git repository (or any of the parent directories): .git\n' >&2
    exit 128
fi

if [ "$1" = "-C" ] && [ "$3" = "config" ] && [ "$4" = "--get" ]; then
    fixtures=${AGENTBOX_TEST_FIXTURES:-}
    key=${5:-}
    if [ "$fixtures" != "" ] && [ -f "$fixtures/git-config-$(safe_git_config_key "$key")" ]; then
        cat "$fixtures/git-config-$(safe_git_config_key "$key")"
        exit 0
    fi

    exit 1
fi

printf 'unsupported git invocation: %s\n' "$*" >&2
exit 1
