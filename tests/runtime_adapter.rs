// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::preflight::{
    CODEX_CONFIG_DESTINATION, ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION,
    NIX_CLIENT_DESTINATION, NIX_STORE_DESTINATION, NixCustomConfPreflightSnapshot,
    OPENCODE_CONFIG_DESTINATION, OPENCODE_DATA_DESTINATION, PreflightSnapshot,
    check_host_prerequisites_with_snapshot, required_host_mount_destinations,
};
use agentbox::runtime::default_image::{
    CODEX_DEFAULT_IMAGE, OPENCODE_DEFAULT_IMAGE as DEFAULT_IMAGE, embedded_default_image_paths,
    materialize_default_image_context,
};
use agentbox::runtime::{AttachEndpoint, RuntimeKind, RuntimeMountKind};
use std::fs;
use std::path::Path;

use agentbox::workspace::resolve_workspace_identity;
use camino::{Utf8Path, Utf8PathBuf};

#[path = "support/mod.rs"]
mod support;

#[test]
fn opencode_create_spec_matches_mvp_contract() {
    let repo = support::temp_git_repo();
    let workspace = resolve_workspace_identity(repo.path()).unwrap();
    let preflight = check_host_prerequisites_with_snapshot(
        &support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Opencode),
        Some(Utf8Path::from_path(repo.path()).unwrap()),
        RuntimeKind::Opencode,
    )
    .unwrap();

    let runtime = RuntimeKind::Opencode;
    let spec = runtime.create_spec(
        &workspace,
        &preflight.host_nix_mounts,
        &preflight.runtime_mounts,
        runtime.server_command().argv,
    );

    assert_eq!(spec.image, DEFAULT_IMAGE);
    assert_eq!(
        spec.labels.get(LABEL_MANAGED),
        Some(&LABEL_MANAGED_VALUE.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_SCHEMA),
        Some(&LABEL_SCHEMA_VALUE.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_GIT_ROOT),
        Some(&workspace.canonical_git_root.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_GIT_ROOT_HASH),
        Some(&workspace.hash12)
    );
    assert_eq!(
        spec.labels.get(LABEL_RUNTIME).map(String::as_str),
        Some(RuntimeKind::Opencode.as_str())
    );
    assert_eq!(
        spec.labels.get(LABEL_IMAGE),
        Some(&DEFAULT_IMAGE.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_LAUNCH_DIRECTORY),
        Some(&workspace.canonical_target.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_LOGICAL_NAME),
        Some(&workspace.container_name)
    );
    assert_eq!(
        spec.labels.get(LABEL_ATTACH_SCHEME),
        Some(&"http".to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_CONTAINER_PORT),
        Some(&"4096".to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_CONTAINER_LISTEN_IP),
        Some(&"0.0.0.0".to_string())
    );
    assert_eq!(
        spec.command,
        vec![
            "opencode".to_string(),
            "serve".to_string(),
            "--hostname".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            "4096".to_string()
        ]
    );
    assert!(spec.network_enabled);
    assert_eq!(spec.published_ports, vec!["127.0.0.1::4096".to_string()]);
    assert_eq!(
        spec.default_env.get("OPENCODE_CONFIG_CONTENT"),
        Some(&r#"{"autoupdate":false}"#.to_string())
    );

    assert_eq!(spec.mounts[0].kind, RuntimeMountKind::Bind);
    assert_eq!(spec.mounts[0].source, workspace.canonical_git_root);
    assert_eq!(spec.mounts[0].destination, workspace.canonical_git_root);
    assert!(!spec.mounts[0].read_only);

    assert_eq!(spec.mounts[1].kind, RuntimeMountKind::Volume);
    assert_eq!(spec.mounts[1].source, workspace.container_name);
    assert_eq!(spec.mounts[1].destination, "/home/user/.cache/nix");
    assert!(!spec.mounts[1].read_only);

    let host_mounts = &spec.mounts[2..6];
    assert_eq!(
        host_mounts
            .iter()
            .map(|mount| mount.destination.as_str())
            .collect::<Vec<_>>(),
        vec![
            NIX_STORE_DESTINATION,
            NIX_CLIENT_DESTINATION,
            ETC_NIX_DESTINATION,
            ETC_STATIC_NIX_DESTINATION,
        ]
    );
    assert!(host_mounts.iter().all(|mount| mount.read_only));
    let opencode_mounts = &spec.mounts[6..];
    assert_eq!(
        opencode_mounts
            .iter()
            .map(|mount| (mount.source.as_str(), mount.destination.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                "/home/example/.config/opencode",
                OPENCODE_CONFIG_DESTINATION,
            ),
            (
                "/home/example/.local/share/opencode",
                OPENCODE_DATA_DESTINATION,
            ),
        ]
    );
    assert!(
        opencode_mounts
            .iter()
            .all(|mount| mount.kind == RuntimeMountKind::Bind && !mount.read_only)
    );
    assert_eq!(
        required_host_mount_destinations().as_slice(),
        [
            NIX_STORE_DESTINATION,
            NIX_CLIENT_DESTINATION,
            ETC_NIX_DESTINATION,
            ETC_STATIC_NIX_DESTINATION,
        ]
    );

    assert_eq!(
        runtime.server_command().argv,
        vec![
            "opencode".to_string(),
            "serve".to_string(),
            "--hostname".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            "4096".to_string()
        ]
    );
}

#[test]
fn opencode_preflight_rejects_unusable_state_directories() {
    let mut snapshot =
        support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Opencode);
    support::host_state_mut(&mut snapshot, OPENCODE_CONFIG_DESTINATION).exists = false;
    assert_opencode_preflight_error(snapshot, "Missing host OpenCode configuration directory");

    let mut snapshot =
        support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Opencode);
    support::host_state_mut(&mut snapshot, OPENCODE_DATA_DESTINATION).is_directory = false;
    assert_opencode_preflight_error(snapshot, "Host OpenCode data path is not a directory");

    let mut snapshot =
        support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Opencode);
    support::host_state_mut(&mut snapshot, OPENCODE_CONFIG_DESTINATION).readable = false;
    assert_opencode_preflight_error(
        snapshot,
        "Host OpenCode configuration directory is not readable and writable",
    );

    let mut snapshot =
        support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Opencode);
    support::host_state_mut(&mut snapshot, OPENCODE_DATA_DESTINATION).writable = false;
    assert_opencode_preflight_error(
        snapshot,
        "Host OpenCode data directory is not readable and writable",
    );
}

#[test]
fn runtime_adapters_render_host_client_commands() {
    let opencode = RuntimeKind::Opencode;
    let opencode_endpoint = AttachEndpoint {
        scheme: "http".to_string(),
        host_ip: "127.0.0.1".to_string(),
        host_port: 49152,
    };
    assert_eq!(
        opencode.host_client_command(&opencode_endpoint).argv,
        vec![
            "opencode".to_string(),
            "attach".to_string(),
            "http://127.0.0.1:49152".to_string(),
        ]
    );

    let codex = RuntimeKind::Codex;
    let codex_endpoint = AttachEndpoint {
        scheme: "ws".to_string(),
        host_ip: "127.0.0.1".to_string(),
        host_port: 49153,
    };
    assert_eq!(
        codex.server_command().argv,
        vec![
            "codex".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "app-server".to_string(),
            "--listen".to_string(),
            "ws://0.0.0.0:1455".to_string(),
        ]
    );
    assert_eq!(
        codex.host_client_command(&codex_endpoint).argv,
        vec![
            "codex".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "--remote".to_string(),
            "ws://127.0.0.1:49153".to_string(),
        ]
    );
}

#[test]
fn codex_create_spec_includes_host_codex_config_mount() {
    let repo = support::temp_git_repo();
    let workspace = resolve_workspace_identity(repo.path()).unwrap();
    let preflight = check_host_prerequisites_with_snapshot(
        &support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Codex),
        Some(Utf8Path::from_path(repo.path()).unwrap()),
        RuntimeKind::Codex,
    )
    .unwrap();

    let spec = RuntimeKind::Codex.create_spec(
        &workspace,
        &preflight.host_nix_mounts,
        &preflight.runtime_mounts,
        RuntimeKind::Codex.server_command().argv,
    );
    let codex_mount = spec
        .mounts
        .iter()
        .find(|mount| mount.destination == CODEX_CONFIG_DESTINATION)
        .unwrap();

    assert_eq!(codex_mount.kind, RuntimeMountKind::Bind);
    assert_eq!(codex_mount.source, "/home/example/.codex");
    assert!(!codex_mount.read_only);
}

#[test]
fn runtime_adapters_own_default_image_references() {
    assert_eq!(RuntimeKind::Opencode.default_image(), DEFAULT_IMAGE);
    assert_eq!(RuntimeKind::Codex.default_image(), CODEX_DEFAULT_IMAGE);
}

#[test]
fn supported_runtime_strings_are_derived_from_profiles() {
    assert_eq!(
        RuntimeKind::supported_values_placeholder(),
        "<opencode|codex>"
    );

    let error = "future-runtime".parse::<RuntimeKind>().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("supported runtimes are `opencode` and `codex`")
    );
}

#[test]
fn preflight_missing_nix_conf_reports_exact_message() {
    let mut snapshot =
        support::passing_preflight_snapshot_with_static_nix_mount(RuntimeKind::Opencode);
    snapshot.nix.config.has_readable_nix_conf = false;
    snapshot.nix.config.custom_conf = NixCustomConfPreflightSnapshot {
        present: false,
        has_readable_target: false,
        needs_static_mount: false,
    };

    let error =
        check_host_prerequisites_with_snapshot(&snapshot, None, RuntimeKind::Opencode).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Missing readable host Nix config: /etc/nix/nix.conf. Mount /etc/nix:/etc/nix:ro."
    );
}

#[test]
fn envrc_above_repo_root_does_not_trigger_direnv_requirement() {
    let sandbox = tempfile::tempdir().unwrap();
    let repo = sandbox.path().join("repo");
    let nested = repo.join("nested");

    fs::create_dir(&repo).unwrap();
    support::init_git_repo(&repo);
    fs::create_dir(&nested).unwrap();
    fs::write(sandbox.path().join(".envrc"), "use nix\n").unwrap();

    let workspace = resolve_workspace_identity(&nested).unwrap();
    let snapshot = PreflightSnapshot::detect(
        Some(workspace.canonical_target.as_ref()),
        Some(workspace.canonical_git_root.as_ref()),
        RuntimeKind::Opencode,
    );

    assert!(!snapshot.host.direnv.required);
}

fn assert_opencode_preflight_error(snapshot: PreflightSnapshot, expected: &str) {
    let error =
        check_host_prerequisites_with_snapshot(&snapshot, None, RuntimeKind::Opencode).unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn materialized_default_image_context_contains_only_required_files() {
    let context = materialize_default_image_context().unwrap();
    let mut files =
        collect_relative_files(context.root().as_std_path(), context.root().as_std_path());
    files.sort();

    let mut expected = embedded_default_image_paths()
        .map(|path| path.to_string())
        .collect::<Vec<_>>();
    expected.sort();

    assert_eq!(files, expected);
}

#[test]
fn materialized_default_image_context_matches_repo_assets() {
    let context = materialize_default_image_context().unwrap();
    let asset_root = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/image");

    for relative_path in embedded_default_image_paths() {
        let materialized = fs::read(context.root().join(relative_path).as_std_path()).unwrap();
        let source = fs::read(asset_root.join(relative_path).as_std_path()).unwrap();
        assert_eq!(materialized, source, "mismatch for {relative_path}");
    }
}

fn collect_relative_files(root: &Path, current: &Path) -> Vec<String> {
    let mut files = Vec::new();

    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_relative_files(root, &path));
            continue;
        }

        let relative = path.strip_prefix(root).unwrap();
        files.push(
            Utf8PathBuf::from_path_buf(relative.to_path_buf())
                .unwrap()
                .to_string(),
        );
    }

    files
}
