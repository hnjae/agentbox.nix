// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::preflight::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, NIX_CLIENT_DESTINATION, NIX_STORE_DESTINATION,
    PreflightSnapshot, check_host_prerequisites_with_snapshot,
};
use agentbox::runtime::RuntimeMountKind;
use agentbox::runtime::opencode::{
    DEFAULT_IMAGE, OpencodeRuntime, RUNTIME_NAME, required_host_mount_destinations,
};
use agentbox::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::workspace::resolve_workspace_identity;
use camino::Utf8Path;

#[path = "support/mod.rs"]
mod support;

#[test]
fn opencode_create_spec_matches_mvp_contract() {
    let repo = support::temp_git_repo();
    let workspace = resolve_workspace_identity(repo.path()).unwrap();
    let preflight = check_host_prerequisites_with_snapshot(
        &PreflightSnapshot {
            has_git: true,
            has_podman: true,
            direnv_required: false,
            has_direnv: false,
            has_nix_daemon_socket: true,
            nix_client_source: Some("/run/current-system/sw/bin/nix".into()),
            has_etc_nix_mount: true,
            has_readable_nix_conf: true,
            nix_custom_conf_present: true,
            has_readable_nix_custom_conf_target: true,
            needs_static_nix_mount: true,
        },
        Some(Utf8Path::from_path(repo.path()).unwrap()),
    )
    .unwrap();

    let runtime = OpencodeRuntime::new();
    let spec = runtime.create_spec(&workspace, None, &preflight);

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
        spec.labels.get(LABEL_RUNTIME),
        Some(&RUNTIME_NAME.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_IMAGE),
        Some(&DEFAULT_IMAGE.to_string())
    );
    assert_eq!(
        spec.labels.get(LABEL_LOGICAL_NAME),
        Some(&workspace.container_name)
    );
    assert_eq!(
        spec.command,
        vec!["sleep".to_string(), "infinity".to_string()]
    );
    assert!(spec.network_enabled);
    assert!(spec.published_ports.is_empty());
    assert!(spec.default_env.is_empty());

    assert_eq!(spec.mounts[0].kind, RuntimeMountKind::Bind);
    assert_eq!(spec.mounts[0].source, workspace.canonical_git_root);
    assert_eq!(spec.mounts[0].destination, workspace.canonical_git_root);
    assert!(!spec.mounts[0].read_only);

    assert_eq!(spec.mounts[1].kind, RuntimeMountKind::Volume);
    assert_eq!(spec.mounts[1].source, workspace.container_name);
    assert_eq!(spec.mounts[1].destination, "/home/user/.cache/nix");
    assert!(!spec.mounts[1].read_only);

    let host_mounts = &spec.mounts[2..];
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
        runtime.detached_server_start().argv,
        vec![
            "/entrypoint",
            "opencode",
            "serve",
            "--hostname",
            "127.0.0.1",
            "--port",
            "4096",
        ]
    );
    assert!(runtime.detached_server_start().detached);
    assert_eq!(
        runtime.health_probe().argv,
        vec![
            "/entrypoint",
            "curl",
            "--max-time",
            "2",
            "-sf",
            "http://127.0.0.1:4096/global/health",
        ]
    );
    assert_eq!(
        runtime
            .attach_command(workspace.canonical_target.as_ref())
            .argv,
        vec![
            "/entrypoint",
            "opencode",
            "attach",
            "http://127.0.0.1:4096",
            "--dir",
            workspace.canonical_target.as_str(),
        ]
    );
}

#[test]
fn preflight_missing_nix_conf_reports_exact_message() {
    let error = check_host_prerequisites_with_snapshot(
        &PreflightSnapshot {
            has_git: true,
            has_podman: true,
            direnv_required: false,
            has_direnv: false,
            has_nix_daemon_socket: true,
            nix_client_source: Some("/run/current-system/sw/bin/nix".into()),
            has_etc_nix_mount: true,
            has_readable_nix_conf: false,
            nix_custom_conf_present: false,
            has_readable_nix_custom_conf_target: false,
            needs_static_nix_mount: false,
        },
        None,
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Missing readable host Nix config: /etc/nix/nix.conf. Mount /etc/nix:/etc/nix:ro."
    );
}
