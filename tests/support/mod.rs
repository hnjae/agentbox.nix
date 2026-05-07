#![allow(dead_code, unused_imports)]

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

mod cli_harness;
mod fake_bins;
mod git_repo;
mod podman_fixtures;
mod preflight_fixtures;
mod ready_endpoint;

pub use cli_harness::CliHarness;
pub use fake_bins::{
    FakeBinDir, fake_git_script, operation_names, path_with_prepend, read_log_lines,
    write_executable,
};
pub use git_repo::{
    TempWorkspace, init_git_repo, temp_git_repo, temp_workspace, tempdir_outside_git,
};
pub use podman_fixtures::{
    cached_managed_inspect_fixture, default_runtime_images_fixture, inspect_models_by_id,
    managed_container_models, managed_container_models_with_hash, managed_inspect_fixture,
    managed_labels, managed_labels_for_image, managed_ps_entry, opencode_managed_labels,
    opencode_workspace_inspect_fixture, opencode_workspace_labels, podman_images_fixture,
    podman_inspect_fixture, podman_ps_fixture, ps_fixture, running_managed_inspect_fixture,
    running_workspace_inspect_fixture, running_workspace_inspect_fixture_with_host_port,
    runtime_image_fixture, volumes_fixture, workspace_ps_entry,
};
pub use preflight_fixtures::{
    host_state_mut, passing_preflight_snapshot_with_static_nix_mount, snapshot_with,
};
pub use ready_endpoint::ReadyEndpoint;
