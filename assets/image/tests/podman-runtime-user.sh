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
    test "$PWD" = /workspace
'

printf 'podman runtime user contract OK\n'
