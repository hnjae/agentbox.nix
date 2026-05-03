// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::preflight::{PreflightSnapshot, check_host_prerequisites_with_snapshot};
use agentbox::session::{LABEL_RUNTIME, REQUIRED_NIX_CACHE_MOUNT_DESTINATION};
use agentbox::workspace::resolve_workspace_identity;
use camino::Utf8Path;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, managed_inspect_fixture, managed_labels, managed_ps_entry, ps_fixture,
};

#[test]
fn required_error_cases_are_actionable() {
    workspace_identity_errors_are_actionable();
    host_preflight_errors_are_actionable();
    attach_side_drift_errors_are_actionable();
    runtime_command_failures_are_actionable();
}

fn workspace_identity_errors_are_actionable() {
    let non_git = support::tempdir_outside_git();
    let error = resolve_workspace_identity(non_git.path()).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains(non_git.path().to_str().unwrap()),
        "{message}"
    );
    assert!(message.contains("git repository"), "{message}");
    assert!(message.contains("git init"), "{message}");

    let escaped_target = Utf8Path::new("/workspace/demo/escape");
    let git_root = Utf8Path::new("/workspace/demo");
    let message = agentbox::Error::escaped_git_target(escaped_target, git_root).to_string();
    assert!(
        message.contains("resolves outside the git root"),
        "{message}"
    );
    assert!(message.contains(escaped_target.as_str()), "{message}");
    assert!(message.contains(git_root.as_str()), "{message}");
}

fn host_preflight_errors_are_actionable() {
    let target = Utf8Path::new("/workspace/demo/nested");

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_git = false),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("`git` was not found on PATH; install `git` or add it to PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_podman = false),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("`podman` was not found on PATH; install `podman` or add it to PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| {
            snapshot.direnv_required = true;
            snapshot.has_direnv = false;
        }),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("`.envrc` applies to `/workspace/demo/nested`")
    );
    assert!(
        error
            .to_string()
            .contains("install `direnv` or add it to PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_nix_daemon_socket = false),
        Some(target),
    )
    .unwrap_err();
    assert!(error.to_string().contains(
        "Missing host nix-daemon socket at: /nix/var/nix/daemon-socket/socket. Mount /nix:/nix:ro."
    ));

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.nix_client_source = None),
        Some(target),
    )
    .unwrap_err();
    assert!(error.to_string().contains("`nix` was not found on PATH"));

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_etc_nix_mount = false),
        Some(target),
    )
    .unwrap_err();
    assert!(error.to_string().contains("Missing /etc/nix host mount"));

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_readable_nix_conf = false),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Missing readable host Nix config: /etc/nix/nix.conf")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| {
            snapshot.nix_custom_conf_present = true;
            snapshot.has_readable_nix_custom_conf_target = false;
        }),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Missing readable target for /etc/nix/nix.custom.conf")
    );
}

fn attach_side_drift_errors_are_actionable() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();

    let missing_labels = Harness::new();
    missing_labels.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-labels-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    let mut labels = managed_labels(
        workspace.canonical_git_root.as_str(),
        &workspace.hash12,
        "opencode",
        &workspace.container_name,
    );
    labels.remove(LABEL_RUNTIME);
    missing_labels.write_inspect(
        "failed-labels-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            true,
            labels,
        ),
    );

    missing_labels
        .attach_assert(&target)
        .failure()
        .stderr(predicates::str::contains("missing required session labels"))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ))
        .stderr(predicates::str::contains(
            "repair or recreate it before retrying",
        ));

    let missing_cache = Harness::new();
    missing_cache.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-cache-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    missing_cache.write_inspect(
        "failed-cache-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    missing_cache
        .attach_assert(&target)
        .failure()
        .stderr(predicates::str::contains("missing required cache mount"))
        .stderr(predicates::str::contains(
            REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
        ))
        .stderr(predicates::str::contains(
            "recreate the container before retrying",
        ));
}

fn runtime_command_failures_are_actionable() {
    let run_failure = RunFailureCase {
        target_subdir: "run-failure",
        failure: FailureSpec::new("run", "container failed to start", 125),
        expected: vec![
            "failed to run the runtime server command",
            "container failed to start",
            "Verify the runtime image still provides `/entrypoint`",
        ],
    };
    assert_run_failure_case(run_failure);

    let missing_ca_bundle = RunFailureCase {
        target_subdir: "missing-ca-bundle",
        failure: FailureSpec::new(
            "run",
            "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt.",
            126,
        ),
        expected: vec![
            "failed to run the runtime server command",
            "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt.",
            "Verify the runtime image still provides `/entrypoint`",
        ],
    };
    assert_run_failure_case(missing_ca_bundle);

    let unusable_state_path = RunFailureCase {
        target_subdir: "unusable-state-path",
        failure: FailureSpec::new(
            "run",
            "Unusable Nix profile state path: /proc/agentbox-state/nix/profile. Ensure XDG_STATE_HOME or HOME points to a writable location.",
            125,
        ),
        expected: vec![
            "failed to run the runtime server command",
            "Unusable Nix profile state path: /proc/agentbox-state/nix/profile",
            "retry or recreate the session",
        ],
    };
    assert_run_failure_case(unusable_state_path);

    let missing_server_command = RunFailureCase {
        target_subdir: "missing-server-command",
        failure: FailureSpec::new("run", "opencode: not found", 127),
        expected: vec![
            "failed to run the runtime server command",
            "opencode: not found",
            "Verify the runtime image still provides `/entrypoint` and the expected runtime tools",
        ],
    };
    assert_run_failure_case(missing_server_command);

    let entrypoint_failure = RunFailureCase {
        target_subdir: "entrypoint-failure",
        failure: FailureSpec::new("run", "/entrypoint: Permission denied", 126),
        expected: vec![
            "failed to run the runtime server command",
            "/entrypoint: Permission denied",
            "retry or recreate the session",
        ],
    };
    assert_run_failure_case(entrypoint_failure);

    let permission_denied = RunFailureCase {
        target_subdir: "workspace-permission-denied",
        failure: FailureSpec::new(
            "run",
            "Permission denied: /tmp/agentbox-denied/workspace-permission-denied",
            13,
        ),
        expected: vec![
            "failed to run the runtime server command",
            "Permission denied: /tmp/agentbox-denied/workspace-permission-denied",
            "retry or recreate the session",
        ],
    };
    assert_run_failure_case(permission_denied);
}

fn assert_run_failure_case(case: RunFailureCase<'_>) {
    let repo = support::temp_git_repo();
    let target = repo.path().join(case.target_subdir);
    fs::create_dir(&target).unwrap();

    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.write_failure(
        case.failure.kind,
        case.failure.stderr,
        case.failure.exit_code,
    );

    let assert = harness.run_assert(&target);
    let mut assert = assert.failure();
    for expected in case.expected {
        assert = assert.stderr(predicates::str::contains(expected));
    }
}

fn snapshot_with(configure: impl FnOnce(&mut PreflightSnapshot)) -> PreflightSnapshot {
    let mut snapshot = PreflightSnapshot {
        has_git: true,
        has_podman: true,
        direnv_required: false,
        has_direnv: true,
        has_nix_daemon_socket: true,
        nix_client_source: Some("/run/current-system/sw/bin/nix".into()),
        has_etc_nix_mount: true,
        has_readable_nix_conf: true,
        nix_custom_conf_present: false,
        has_readable_nix_custom_conf_target: true,
        needs_static_nix_mount: false,
    };
    configure(&mut snapshot);
    snapshot
}

struct RunFailureCase<'a> {
    target_subdir: &'a str,
    failure: FailureSpec<'a>,
    expected: Vec<&'a str>,
}

struct FailureSpec<'a> {
    kind: &'a str,
    stderr: &'a str,
    exit_code: i32,
}

impl<'a> FailureSpec<'a> {
    fn new(kind: &'a str, stderr: &'a str, exit_code: i32) -> Self {
        Self {
            kind,
            stderr,
            exit_code,
        }
    }
}
