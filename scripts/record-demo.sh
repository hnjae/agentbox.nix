#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

set -eu

usage() {
    cat <<'EOF'
Usage: scripts/record-demo.sh [OPTIONS] [OUTPUT.cast]

Record an automated agentbox terminal demo with asciinema.

Options:
    --runtime RUNTIME      Runtime to demo: codex or opencode (default: codex)
    --workspace PATH       Existing git workspace to use instead of a temporary one
    --agentbox PATH        agentbox executable to record (default: local cargo build)
    --connect              Connect to the session and send /exit before stopping it
    --render-media         Require and generate @2x.gif and @2x.avif outputs
    --no-render-media      Do not generate media outputs
    -o, --output PATH      Cast output path
    -h, --help             Show this help

Environment:
    AGENTBOX_DEMO_RUNTIME  Same as --runtime
    AGENTBOX_DEMO_WORKSPACE
                            Same as --workspace
    AGENTBOX_BIN           Same as --agentbox
    AGENTBOX_DEMO_CONNECT  Same as --connect when set to 1
    AGENTBOX_DEMO_CONNECT_DELAY
                            Seconds to wait before sending /exit (default: 2)
    AGENTBOX_DEMO_RENDER_MEDIA
                            1, 0, or auto (default: auto)
    AGENTBOX_DEMO_MEDIA_FONT_SIZE
                            agg font size for @2x output (default: 32)
    AGENTBOX_DEMO_MEDIA_FPS_CAP
                            agg FPS cap for @2x output (default: 20)
    AGENTBOX_DEMO_MEDIA_IDLE_TIME_LIMIT
                            agg idle time limit for @2x output (default: 1)
    AGENTBOX_DEMO_MEDIA_AVIF_CRF
                            ffmpeg libaom-av1 CRF for @2x AVIF (default: 28)
    AGENTBOX_DEMO_MEDIA_AVIF_CPU_USED
                            ffmpeg libaom-av1 cpu-used for @2x AVIF (default: 4)
    AGENTBOX_DEMO_PAUSE    Delay between demo steps in seconds (default: 0.8)
EOF
}

error() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || error "required command not found on PATH: $1"
}

has_command() {
    command -v "$1" >/dev/null 2>&1
}

absolute_script_path() {
    case $0 in
    */*)
        script_dir=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
        printf '%s/%s\n' "$script_dir" "${0##*/}"
        ;;
    *)
        command_path=$(command -v "$0") || error "cannot resolve script path: $0"
        case $command_path in
        */*)
            script_dir=$(CDPATH='' cd "$(dirname "$command_path")" && pwd -P)
            printf '%s/%s\n' "$script_dir" "${command_path##*/}"
            ;;
        *) error "cannot resolve script path: $0" ;;
        esac
        ;;
    esac
}

runtime_state_source() {
    runtime=$1

    case $runtime in
    codex)
        if [ "${CODEX_HOME-}" != "" ]; then
            printf '%s\n' "$CODEX_HOME"
        elif [ "${HOME-}" != "" ]; then
            printf '%s/.codex\n' "$HOME"
        else
            error "HOME is not set and CODEX_HOME was not provided"
        fi
        ;;
    opencode)
        if [ "${XDG_CONFIG_HOME-}" != "" ]; then
            printf '%s/opencode\n' "$XDG_CONFIG_HOME"
        elif [ "${HOME-}" != "" ]; then
            printf '%s/.config/opencode\n' "$HOME"
        else
            error "HOME is not set and XDG_CONFIG_HOME was not provided"
        fi
        ;;
    *) error "unsupported runtime: $runtime" ;;
    esac
}

preflight_runtime_state() {
    runtime=$1
    source=$(runtime_state_source "$runtime")

    case $source in
    /*) ;;
    *) error "$runtime state path must be absolute: $source" ;;
    esac

    if [ ! -d "$source" ]; then
        error "$runtime state directory does not exist: $source"
    fi

    if [ "$runtime" = opencode ]; then
        if [ "${XDG_DATA_HOME-}" != "" ]; then
            data_source=$XDG_DATA_HOME/opencode
        elif [ "${HOME-}" != "" ]; then
            data_source=$HOME/.local/share/opencode
        else
            error "HOME is not set and XDG_DATA_HOME was not provided"
        fi

        case $data_source in
        /*) ;;
        *) error "opencode data path must be absolute: $data_source" ;;
        esac

        if [ ! -d "$data_source" ]; then
            error "opencode data directory does not exist: $data_source"
        fi
    fi
}

prepare_agentbox_binary() {
    repo_root=$1
    agentbox_bin=$2

    if [ "$agentbox_bin" != "" ]; then
        case $agentbox_bin in
        */*) ;;
        *)
            requested_agentbox_bin=$agentbox_bin
            agentbox_bin=$(command -v "$requested_agentbox_bin") || error "agentbox executable not found: $requested_agentbox_bin"
            ;;
        esac

        if [ ! -x "$agentbox_bin" ]; then
            error "agentbox executable is not executable: $agentbox_bin"
        fi

        printf '%s\n' "$agentbox_bin"
        return
    fi

    require_command cargo
    printf 'Building local agentbox binary...\n' >&2
    cargo build --quiet --bin agentbox
    agentbox_bin=$repo_root/target/debug/agentbox

    if [ ! -x "$agentbox_bin" ]; then
        error "local build did not produce executable: $agentbox_bin"
    fi

    printf '%s\n' "$agentbox_bin"
}

media_output_prefix() {
    output=$1
    output_dir=$(dirname "$output")
    output_name=$(basename "$output")

    case $output_name in
    *.cast) output_stem=${output_name%.cast} ;;
    *.*) output_stem=${output_name%.*} ;;
    *) output_stem=$output_name ;;
    esac

    printf '%s/%s' "$output_dir" "$output_stem"
}

media_output_paths() {
    output=$1
    output_prefix=$(media_output_prefix "$output")

    printf '%s@2x.gif\n' "$output_prefix"
    printf '%s@2x.avif\n' "$output_prefix"
}

check_media_outputs() {
    output=$1

    media_output_paths "$output" | while IFS= read -r media_output; do
        if [ -e "$media_output" ]; then
            error "media output file already exists: $media_output"
        fi
    done
}

preflight_media_rendering() {
    require_command agg
    require_command ffmpeg

    ffmpeg -hide_banner -h encoder=libaom-av1 >/dev/null 2>&1 || error "ffmpeg does not support the libaom-av1 encoder"
    ffmpeg -hide_banner -h muxer=avif >/dev/null 2>&1 || error "ffmpeg does not support the avif muxer"
}

render_demo_media() {
    output=$1
    output_prefix=$(media_output_prefix "$output")
    gif_output=$output_prefix@2x.gif
    avif_output=$output_prefix@2x.avif

    printf 'Rendering @2x GIF demo to %s\n' "$gif_output"
    agg \
        --font-size "$AGENTBOX_DEMO_MEDIA_FONT_SIZE" \
        --fps-cap "$AGENTBOX_DEMO_MEDIA_FPS_CAP" \
        --idle-time-limit "$AGENTBOX_DEMO_MEDIA_IDLE_TIME_LIMIT" \
        "$output" \
        "$gif_output"

    printf 'Rendering @2x AVIF demo to %s\n' "$avif_output"
    ffmpeg \
        -hide_banner \
        -loglevel error \
        -n \
        -i "$gif_output" \
        -an \
        -c:v libaom-av1 \
        -crf "$AGENTBOX_DEMO_MEDIA_AVIF_CRF" \
        -cpu-used "$AGENTBOX_DEMO_MEDIA_AVIF_CPU_USED" \
        -pix_fmt yuv444p \
        -loop 0 \
        "$avif_output"
}

run_step() {
    label=$1
    shift

    printf '\n\033[1;36m$ %s\033[0m\n' "$label"
    sleep "$AGENTBOX_DEMO_PAUSE"

    set +e
    "$@"
    status=$?
    set -e

    if [ "$status" -ne 0 ]; then
        printf '\ncommand failed with exit status %s\n' "$status" >&2
        return "$status"
    fi

    sleep "$AGENTBOX_DEMO_PAUSE"
}

run_connect_exit_step() {
    workspace=$1

    printf '\n\033[1;36m$ agentbox connect %s\033[0m\n' "$workspace"
    sleep "$AGENTBOX_DEMO_PAUSE"

    set +e
    # shellcheck disable=SC2016
    env \
        AGENTBOX_DEMO_CONNECT_WORKSPACE="$workspace" \
        AGENTBOX_DEMO_CONNECT_AGENTBOX_BIN="$AGENTBOX_DEMO_AGENTBOX_BIN" \
        AGENTBOX_DEMO_CONNECT_DELAY_SECONDS="$AGENTBOX_DEMO_CONNECT_DELAY" \
        expect -c '
        set timeout 60
        set workspace $env(AGENTBOX_DEMO_CONNECT_WORKSPACE)
        set agentbox_bin $env(AGENTBOX_DEMO_CONNECT_AGENTBOX_BIN)
        set exit_delay_ms [expr {int(double($env(AGENTBOX_DEMO_CONNECT_DELAY_SECONDS)) * 1000)}]

        spawn -noecho $agentbox_bin connect $workspace
        after $exit_delay_ms
        send -- "/exit"

        set exited 0
        foreach submit_delay_ms {500 1500 3000} {
            after $submit_delay_ms
            send -- "\r"
            set timeout 2
            expect {
                eof {
                    set exited 1
                    break
                }
                timeout {}
            }
        }

        if {!$exited} {
            set timeout 60
            expect {
                eof {}
                timeout {
                    send -- "\003"
                    exit 124
                }
            }
        }

        set wait_result [wait]
        if {[lindex $wait_result 2] != 0} {
            exit 1
        }
        exit [lindex $wait_result 3]
    '
    status=$?
    set -e

    if [ "$status" -ne 0 ]; then
        printf '\nconnect demo failed with exit status %s\n' "$status" >&2
        return "$status"
    fi

    sleep "$AGENTBOX_DEMO_PAUSE"
}

demo_session_id() {
    workspace=$1
    runtime=$2

    sessions_json=$("$AGENTBOX_DEMO_AGENTBOX_BIN" ls --output json)
    session_ids=$(
        printf '%s\n' "$sessions_json" |
            sed 's/},{/}\
{/g' |
            awk -v workspace="$workspace" -v runtime="$runtime" '
                index($0, "\"type\":\"managed\"") &&
                index($0, "\"canonical_git_root\":\"" workspace "\"") &&
                index($0, "\"runtime\":\"" runtime "\"") &&
                index($0, "\"status\":\"running\"") {
                    if (match($0, /"id":"[^"]+"/)) {
                        print substr($0, RSTART + 6, RLENGTH - 7)
                    }
                }
            '
    )

    session_count=$(printf '%s\n' "$session_ids" | awk 'NF { count++ } END { print count + 0 }')
    case $session_count in
    1) printf '%s\n' "$session_ids" ;;
    0) error "could not find running $runtime demo session for workspace: $workspace" ;;
    *) error "multiple running $runtime demo sessions found for workspace: $workspace" ;;
    esac
}

prepare_demo_workspace() {
    repo_root=$1
    workspace=$2

    if [ "$workspace" != "" ]; then
        if [ ! -d "$workspace" ]; then
            error "workspace directory does not exist: $workspace"
        fi

        git -C "$workspace" rev-parse --show-toplevel >/dev/null 2>&1 || error "workspace is not inside a git repository: $workspace"
        printf '%s\n' "$workspace"
        return
    fi

    workspace_parent=$repo_root/target
    mkdir -p "$workspace_parent"
    workspace=$(mktemp -d "$workspace_parent/agentbox-demo.XXXXXX")
    git init -q "$workspace"
    printf '# agentbox demo\n' >"$workspace/README.md"
    printf '%s\n' "$workspace"
}

cleanup_demo_workspace() {
    if [ "${AGENTBOX_DEMO_AGENTBOX_BIN-}" != "" ] && [ "${AGENTBOX_DEMO_WORKSPACE_RESOLVED-}" != "" ]; then
        "$AGENTBOX_DEMO_AGENTBOX_BIN" stop "$AGENTBOX_DEMO_WORKSPACE_RESOLVED" >/dev/null 2>&1 || true
    fi

    if [ "${AGENTBOX_DEMO_OWNS_WORKSPACE-}" = 1 ] && [ "${AGENTBOX_DEMO_WORKSPACE_RESOLVED-}" != "" ]; then
        rm -rf "$AGENTBOX_DEMO_WORKSPACE_RESOLVED"
    fi
}

demo_main() {
    require_command git

    : "${AGENTBOX_DEMO_AGENTBOX_BIN:?}"
    : "${AGENTBOX_DEMO_REPO_ROOT:?}"
    : "${AGENTBOX_DEMO_RUNTIME:?}"
    : "${AGENTBOX_DEMO_PAUSE:=0.8}"
    : "${AGENTBOX_DEMO_CONNECT:=0}"
    : "${AGENTBOX_DEMO_CONNECT_DELAY:=2}"

    workspace=$(prepare_demo_workspace "$AGENTBOX_DEMO_REPO_ROOT" "${AGENTBOX_DEMO_WORKSPACE-}")
    AGENTBOX_DEMO_WORKSPACE_RESOLVED=$workspace
    if [ "${AGENTBOX_DEMO_WORKSPACE-}" != "" ]; then
        AGENTBOX_DEMO_OWNS_WORKSPACE=0
    else
        AGENTBOX_DEMO_OWNS_WORKSPACE=1
    fi
    export AGENTBOX_DEMO_WORKSPACE_RESOLVED AGENTBOX_DEMO_OWNS_WORKSPACE
    trap cleanup_demo_workspace EXIT INT TERM HUP

    printf '\033[2J\033[H'
    printf 'agentbox demo\n'
    printf 'runtime: %s\n' "$AGENTBOX_DEMO_RUNTIME"
    printf 'workspace: %s\n' "$workspace"

    run_step 'agentbox --version' "$AGENTBOX_DEMO_AGENTBOX_BIN" --version
    run_step "agentbox start --runtime $AGENTBOX_DEMO_RUNTIME --dev-env none $workspace" "$AGENTBOX_DEMO_AGENTBOX_BIN" start --runtime "$AGENTBOX_DEMO_RUNTIME" --dev-env none "$workspace"
    run_step 'agentbox ls' "$AGENTBOX_DEMO_AGENTBOX_BIN" ls
    session_id=$(demo_session_id "$workspace" "$AGENTBOX_DEMO_RUNTIME")
    run_step "agentbox health $session_id" "$AGENTBOX_DEMO_AGENTBOX_BIN" health "$session_id"
    if [ "$AGENTBOX_DEMO_CONNECT" = 1 ]; then
        run_connect_exit_step "$workspace"
    fi
    run_step "agentbox stop $session_id" "$AGENTBOX_DEMO_AGENTBOX_BIN" stop "$session_id"

    printf '\nDone.\n'
    sleep "$AGENTBOX_DEMO_PAUSE"
}

main() {
    script_path=$(absolute_script_path)
    script_dir=$(CDPATH='' cd "$(dirname "$script_path")" && pwd -P)
    repo_root=$(CDPATH='' cd "$script_dir/.." && pwd -P)

    if [ "${1-}" = --demo ]; then
        demo_main
        exit 0
    fi

    output=${AGENTBOX_DEMO_OUTPUT-}
    runtime=${AGENTBOX_DEMO_RUNTIME:-codex}
    workspace=${AGENTBOX_DEMO_WORKSPACE-}
    agentbox_bin=${AGENTBOX_BIN-}
    connect_exit=${AGENTBOX_DEMO_CONNECT:-0}
    render_media=${AGENTBOX_DEMO_RENDER_MEDIA:-auto}
    AGENTBOX_DEMO_PAUSE=${AGENTBOX_DEMO_PAUSE:-0.8}
    AGENTBOX_DEMO_CONNECT_DELAY=${AGENTBOX_DEMO_CONNECT_DELAY:-2}
    AGENTBOX_DEMO_MEDIA_FONT_SIZE=${AGENTBOX_DEMO_MEDIA_FONT_SIZE:-32}
    AGENTBOX_DEMO_MEDIA_FPS_CAP=${AGENTBOX_DEMO_MEDIA_FPS_CAP:-20}
    AGENTBOX_DEMO_MEDIA_IDLE_TIME_LIMIT=${AGENTBOX_DEMO_MEDIA_IDLE_TIME_LIMIT:-1}
    AGENTBOX_DEMO_MEDIA_AVIF_CRF=${AGENTBOX_DEMO_MEDIA_AVIF_CRF:-28}
    AGENTBOX_DEMO_MEDIA_AVIF_CPU_USED=${AGENTBOX_DEMO_MEDIA_AVIF_CPU_USED:-4}

    while [ "$#" -gt 0 ]; do
        case $1 in
        --runtime)
            shift
            [ "$#" -gt 0 ] || error "--runtime requires a value"
            runtime=$1
            ;;
        --runtime=*) runtime=${1#--runtime=} ;;
        --workspace)
            shift
            [ "$#" -gt 0 ] || error "--workspace requires a value"
            workspace=$1
            ;;
        --workspace=*) workspace=${1#--workspace=} ;;
        --agentbox)
            shift
            [ "$#" -gt 0 ] || error "--agentbox requires a value"
            agentbox_bin=$1
            ;;
        --agentbox=*) agentbox_bin=${1#--agentbox=} ;;
        --connect) connect_exit=1 ;;
        --render-media) render_media=1 ;;
        --no-render-media) render_media=0 ;;
        -o | --output)
            option=$1
            shift
            [ "$#" -gt 0 ] || error "$option requires a value"
            output=$1
            ;;
        --output=*) output=${1#--output=} ;;
        -h | --help)
            usage
            exit 0
            ;;
        -*)
            error "unknown option: $1"
            ;;
        *)
            if [ "$output" != "" ]; then
                error "unexpected extra argument: $1"
            fi
            output=$1
            ;;
        esac
        shift
    done

    case $runtime in
    codex | opencode) ;;
    *) error "unsupported runtime: $runtime" ;;
    esac
    case $connect_exit in
    0 | 1) ;;
    *) error "AGENTBOX_DEMO_CONNECT must be 0 or 1" ;;
    esac
    case $render_media in
    auto)
        if has_command agg && has_command ffmpeg; then
            render_media=1
        else
            render_media=0
        fi
        ;;
    0 | 1) ;;
    *) error "AGENTBOX_DEMO_RENDER_MEDIA must be 0, 1, or auto" ;;
    esac

    require_command asciinema
    require_command git
    if [ "$connect_exit" = 1 ]; then
        require_command expect
    fi
    if [ "$render_media" = 1 ]; then
        preflight_media_rendering
    fi
    preflight_runtime_state "$runtime"
    agentbox_bin=$(prepare_agentbox_binary "$repo_root" "$agentbox_bin")

    if [ "$output" = "" ]; then
        output=$repo_root/target/asciinema/agentbox-demo-$(date +%Y%m%d-%H%M%S).cast
    fi

    output_dir=$(dirname "$output")
    mkdir -p "$output_dir"
    if [ -e "$output" ]; then
        error "output file already exists: $output"
    fi
    if [ "$render_media" = 1 ]; then
        check_media_outputs "$output"
    fi

    AGENTBOX_DEMO_SCRIPT=$script_path
    AGENTBOX_DEMO_AGENTBOX_BIN=$agentbox_bin
    AGENTBOX_DEMO_REPO_ROOT=$repo_root
    AGENTBOX_DEMO_RUNTIME=$runtime
    AGENTBOX_DEMO_WORKSPACE=$workspace
    AGENTBOX_DEMO_CONNECT=$connect_exit
    export AGENTBOX_DEMO_SCRIPT AGENTBOX_DEMO_AGENTBOX_BIN AGENTBOX_DEMO_REPO_ROOT AGENTBOX_DEMO_RUNTIME AGENTBOX_DEMO_WORKSPACE AGENTBOX_DEMO_PAUSE AGENTBOX_DEMO_CONNECT AGENTBOX_DEMO_CONNECT_DELAY

    printf 'Recording agentbox demo to %s\n' "$output"
    # shellcheck disable=SC2016
    record_command='sh "$AGENTBOX_DEMO_SCRIPT" --demo'
    asciinema rec --return -c "$record_command" "$output"
    printf 'Recorded agentbox demo: %s\n' "$output"
    if [ "$render_media" = 1 ]; then
        render_demo_media "$output"
    fi
}

main "$@"
