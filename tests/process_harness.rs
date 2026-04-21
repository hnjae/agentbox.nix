// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::direnv::Direnv;
use agentbox::git::Git;
use agentbox::podman::Podman;
use agentbox::process::ProcessRunner;
use camino::Utf8Path;

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
    fake_bins.install_exact_failure("direnv", &["export", "json"], "direnv denied access", 7);

    let direnv = Direnv::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()));
    let error = direnv.export_json(Utf8Path::new("/tmp")).unwrap_err();

    assert!(error.to_string().contains("direnv export json"));
    assert!(error.to_string().contains("exit status 7"));
    assert!(error.to_string().contains("direnv denied access"));
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
    let repo = support::temp_git_repo();
    let repo_path = Utf8Path::from_path(repo.path()).unwrap();

    let root = Git::new().rev_parse_show_toplevel(repo_path).unwrap();

    assert_eq!(root, repo_path);
}

#[test]
fn direnv_parses_json_from_a_fake_binary() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "direnv",
        &["export", "json"],
        r#"{"FOO":"bar","REMOVED":null}"#,
    );

    let environment = Direnv::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .export_json(Utf8Path::new("/tmp"))
        .unwrap();

    assert_eq!(
        environment.entries.get("FOO"),
        Some(&Some("bar".to_string()))
    );
    assert_eq!(environment.entries.get("REMOVED"), Some(&None));
}

#[test]
fn podman_ps_parses_stable_json_fields_from_a_fake_binary() {
    let fake_bins = support::FakeBinDir::new();
    fake_bins.install_exact_response(
        "podman",
        &["ps", "--format", "json"],
        support::podman_ps_fixture(),
    );

    let containers = Podman::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()))
        .ps()
        .unwrap();

    assert_eq!(containers.len(), 1);
    let container = &containers[0];
    assert_eq!(container.image, "ghcr.io/example/agentbox:latest");
    assert_eq!(container.command, vec!["/usr/bin/sleep", "infinity"]);
    assert_eq!(container.created, 1713681300);
    assert_eq!(container.created_at, "2026-04-21 10:15:00 +0000 UTC");
    assert_eq!(container.names, vec!["agentbox-demo"]);
    assert_eq!(container.ports[0].host_port, Some(49153));
    assert_eq!(
        container.labels.get("io.containers.autoupdate"),
        Some(&"registry".to_string())
    );
    assert_eq!(container.networks, vec!["podman"]);
    assert_eq!(
        container.namespaces.as_ref().unwrap().net,
        Some("ns:/proc/4321/ns/net".to_string())
    );
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
    assert_eq!(container.path, "/usr/bin/sleep");
    assert_eq!(container.args, vec!["infinity"]);
    assert_eq!(container.state.status, "running");
    assert_eq!(container.state.health.as_ref().unwrap().status, "healthy");
    assert_eq!(container.image_name, "ghcr.io/example/agentbox:latest");
    assert_eq!(
        container.config.entrypoint,
        Some(vec!["/usr/bin/sleep".to_string()])
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
