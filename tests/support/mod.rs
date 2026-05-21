#![allow(dead_code, unused_imports)]

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod cli_harness;
mod command_log;
mod fake_bins;
mod git_repo;
mod podman_fixtures;
mod preflight_fixtures;
mod ready_endpoint;

pub use cli_harness::CliHarness;
pub use command_log::{CommandLog, CommandLogEntry, operation_names};
pub use fake_bins::{
    FakeBinDir, fake_git_script, path_with_prepend, read_log_lines, write_executable,
};
pub use git_repo::{
    TempWorkspace, init_git_repo, temp_git_repo, temp_workspace, tempdir_outside_git,
    write_envrc_at,
};
pub use podman_fixtures::{
    cached_managed_inspect_fixture, default_runtime_images_fixture, inspect_models_by_id,
    managed_container_models, managed_container_models_with_hash, managed_inspect_fixture,
    managed_labels, managed_labels_for_image, managed_ps_entry, opencode_managed_labels,
    opencode_transient_run_labels, opencode_workspace_inspect_fixture,
    opencode_workspace_inspect_fixture_with_cache_bind, opencode_workspace_labels,
    podman_images_fixture, podman_inspect_fixture, podman_ps_fixture, ps_fixture,
    running_managed_inspect_fixture, running_workspace_inspect_fixture,
    running_workspace_inspect_fixture_with_host_port, runtime_image_fixture,
    transient_run_container_models, transient_run_ps_entry, volumes_fixture, workspace_ps_entry,
};
pub use preflight_fixtures::{
    host_state_mut, passing_preflight_snapshot_with_static_nix_mount, snapshot_with,
};
pub use ready_endpoint::ReadyEndpoint;
