#!/bin/sh
# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

# shellcheck disable=SC1090,SC1091

agentbox_runtime_contract=/usr/local/share/libexec/agentbox/lib/runtime-contract.sh

if [ -r "$agentbox_runtime_contract" ]; then
    . "$agentbox_runtime_contract"
    resolve_runtime_paths
    activate_profile_env
fi

unset agentbox_runtime_contract
