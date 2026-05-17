#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

. "$(dirname "$0")/_runtime-contract-helpers.sh"

repo_root=$(runtime_contract_repo_root "$0")

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
