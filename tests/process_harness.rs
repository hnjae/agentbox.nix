// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::git::Git;
use agentbox::podman::{Podman, PodmanBuildOptions};
use agentbox::process::ProcessRunner;
use agentbox::runtime::default_image::OPENCODE_DEFAULT_IMAGE as DEFAULT_IMAGE;
use camino::Utf8Path;
use std::fs;

mod support;

#[test]
fn missing_binaries_are_reported_with_path_guidance() {
    let error = ProcessRunner::new()
        .capture("agentbox-missing-binary-for-test", |_| {})
        .unwrap_err();

    assert!(error.to_string().contains("was not found on PATH"));
    assert!(
        error
            .to_string()
            .contains("install `agentbox-missing-binary-for-test`")
    );
}

#[test]
fn nonzero_exits_include_status_and_stderr() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_failure("failing-tool", &["boom"], "tool denied access", 7);

    let error = ProcessRunner::new()
        .with_path_prepend(fake_bins.path())
        .capture("failing-tool", |command| {
            command.arg("boom");
        })
        .unwrap_err();

    assert!(error.to_string().contains("failing-tool boom"));
    assert!(error.to_string().contains("exit status 7"));
    assert!(error.to_string().contains("tool denied access"));
}

#[test]
fn git_can_use_a_path_injected_fake_binary() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "git",
        &["-C", "/tmp/fake-repo", "rev-parse", "--show-toplevel"],
        "/tmp/fake-repo\n",
    );

    let git = Git::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()));
    let root = git
        .rev_parse_show_toplevel(Utf8Path::new("/tmp/fake-repo"))
        .unwrap();

    assert_eq!(root, Utf8Path::new("/tmp/fake-repo"));
}

#[test]
fn shared_temp_git_repo_supports_real_git_smoke_tests() {
    let fake_bins = support::FakeBinDir::new();
    let repo = support::temp_git_repo();
    let repo_path = Utf8Path::from_path(repo.path()).unwrap();
    fake_bins.install_exact_response(
        "git",
        &["-C", repo_path.as_str(), "rev-parse", "--show-toplevel"],
        &format!("{repo_path}\n"),
    );

    let root = Git::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .rev_parse_show_toplevel(repo_path)
        .unwrap();

    assert_eq!(root, repo_path);
}

#[test]
fn podman_ps_parses_stable_json_fields_from_a_fake_binary() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "podman",
        &[
            "ps",
            "--all",
            "--filter",
            "label=io.agentbox.managed=true",
            "--format",
            "json",
        ],
        support::podman_ps_fixture(),
    );

    let containers = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .ps()
        .unwrap();

    assert_eq!(containers.len(), 1);
    let container = &containers[0];
    assert_eq!(container.image, "ghcr.io/example/agentbox:latest");
    assert_eq!(container.command, None);
    assert_eq!(container.created, 1713681300);
    assert_eq!(container.created_at, "2026-04-21 10:15:00 +0000 UTC");
    assert_eq!(container.names, None);
    assert_eq!(container.ports, None);
    assert_eq!(
        container.labels.get("io.containers.autoupdate"),
        Some(&"registry".to_string())
    );
    assert_eq!(container.networks, None);
    assert_eq!(container.namespaces, None);
}

#[test]
fn podman_inspect_parses_stable_json_fields_from_a_fake_binary() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "podman",
        &["inspect", "agentbox-demo"],
        support::podman_inspect_fixture(),
    );

    let container = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .inspect_one("agentbox-demo")
        .unwrap();

    assert_eq!(container.id.len(), 64);
    assert_eq!(container.path, "/usr/bin/opencode");
    assert!(container.args.is_empty());
    assert_eq!(container.state.status, "running");
    assert_eq!(container.state.health.as_ref().unwrap().status, "healthy");
    assert_eq!(container.image_name, "ghcr.io/example/agentbox:latest");
    assert_eq!(
        container.config.entrypoint,
        Some(vec!["/entrypoint".to_string()])
    );
    assert_eq!(container.config.stop_signal.as_deref(), Some("SIGTERM"));
    assert_eq!(container.mounts[0].destination, "/workspace");
    assert_eq!(
        container
            .network_settings
            .networks
            .get("podman")
            .unwrap()
            .ip_address
            .as_deref(),
        Some("10.88.0.10")
    );
}

#[test]
fn podman_image_exists_treats_exit_status_one_as_missing() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_failure("podman", &["image", "exists", DEFAULT_IMAGE], "", 1);

    let exists = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .image_exists(DEFAULT_IMAGE)
        .unwrap();

    assert!(!exists);
}

#[test]
fn podman_image_exists_returns_true_when_podman_reports_local_presence() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response("podman", &["image", "exists", DEFAULT_IMAGE], "");

    let exists = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .image_exists(DEFAULT_IMAGE)
        .unwrap();

    assert!(exists);
}

#[test]
fn podman_container_exists_treats_exit_status_one_as_missing() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_failure("podman", &["container", "exists", "agentbox-demo"], "", 1);

    let exists = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .container_exists("agentbox-demo")
        .unwrap();

    assert!(!exists);
}

#[test]
fn podman_container_exists_returns_true_when_podman_reports_presence() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response("podman", &["container", "exists", "agentbox-demo"], "");

    let exists = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .container_exists("agentbox-demo")
        .unwrap();

    assert!(exists);
}

#[test]
fn podman_build_image_uses_containerfile_and_context_arguments() {
    let fake_bins = support::FakeBinDir::new();
    let sandbox = tempfile::tempdir().unwrap();
    let context = Utf8Path::from_path(sandbox.path())
        .unwrap()
        .join("build-context");
    fs::create_dir_all(context.as_std_path()).unwrap();
    let containerfile = context.join("Containerfile");
    fs::write(containerfile.as_std_path(), "FROM scratch\n").unwrap();

    fake_bins.install_exact_response(
        "podman",
        &[
            "build",
            "-t",
            DEFAULT_IMAGE,
            "-f",
            containerfile.as_str(),
            context.as_str(),
        ],
        "Successfully built\n",
    );

    Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .build_image(
            DEFAULT_IMAGE,
            containerfile.as_ref(),
            context.as_ref(),
            &PodmanBuildOptions::default(),
        )
        .unwrap();
}

#[test]
fn podman_stop_ignore_uses_the_adapter_runner() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "podman",
        &["stop", "--ignore", "agentbox-demo"],
        "agentbox-demo\n",
    );

    Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .stop_ignore("agentbox-demo")
        .unwrap();
}

#[test]
fn podman_logs_tail_returns_container_output() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "podman",
        &["logs", "--tail", "80", "agentbox-demo"],
        "runtime failed\n",
    );

    let logs = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .logs_tail("agentbox-demo", 80)
        .unwrap();

    assert_eq!(logs, "runtime failed\n");
}
