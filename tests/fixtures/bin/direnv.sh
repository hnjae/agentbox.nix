#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

log_path=${AGENTBOX_TEST_LOG:-}
if [ "$log_path" != "" ]; then
    printf 'direnv args=%s cwd=%s\n' "$*" "$PWD" >>"$log_path"
fi

case "${1:-}" in
exec)
    shift
    directory=${1:?missing direnv exec directory}
    shift
    cd "$directory"
    exec "$@"
    ;;
export)
    printf '{}\n'
    ;;
*)
    exit 0
    ;;
esac
