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

image_tag=containerfile-nixkpgs-runtime-user-contract
container_name=containerfile-nixkpgs-runtime-user-contract-$$

cleanup() {
    runtime_contract_cleanup
}

trap cleanup EXIT INT TERM HUP

runtime_contract_require_command podman
runtime_contract_build_image

podman run --rm --entrypoint /bin/sh --name "$container_name" "$image_tag" -ceu '
    test "$(id -un)" = user
    test "$HOME" = /home/user
    test -d "$HOME"
    test -w "$HOME"
    test -w "$HOME/.cache"
    test -w "$HOME/.config"
    test -w "$HOME/.local"
    test -w "$HOME/.local/state"
    command -v opencode >/dev/null 2>&1
    test -s /usr/local/share/agentbox/opencode.version
    test "$(cat /usr/local/share/agentbox/codex.version)" = not-installed
    test "$PWD" = /workspace
'

printf 'podman runtime user contract OK\n'
