#!/bin/sh
set -eu

log_path=${AGENTBOX_TEST_LOG:-}
if [ -n "$log_path" ]; then
    printf 'direnv args=%s cwd=%s\n' "$*" "$(pwd)" >> "$log_path"
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
