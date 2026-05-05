#!/bin/sh
set -eu

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
