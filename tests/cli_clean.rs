// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::prompt;
use agentbox::runtime::RuntimeKind;
use agentbox::runtime::default_image::default_image_context_hash;
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, managed_inspect_fixture, managed_ps_entry,
    opencode_managed_labels as managed_labels, operation_names, podman_images_fixture, ps_fixture,
    running_workspace_inspect_fixture_with_host_port, runtime_image_fixture, volumes_fixture,
};

const UNUSED_VOLUME: &str = "agentbox-unused-abcdef123456";
const USED_VOLUME: &str = "agentbox-used-abcdef123456";

#[test]
fn clean_dry_run_prints_candidates_without_deleting() {
    let harness = Harness::new();
    let opencode_image = RuntimeKind::Opencode.default_image();
    let codex_image = RuntimeKind::Codex.default_image();
    harness.write_volumes(&volumes_fixture(&[UNUSED_VOLUME]));

    harness
        .agentbox_assert(&["clean", "--dry-run"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("INFO: cleanup candidates:"))
        .stderr(predicate::str::contains(opencode_image.clone()))
        .stderr(predicate::str::contains(codex_image.clone()))
        .stderr(predicate::str::contains(UNUSED_VOLUME));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "volume"]);
    assert!(
        !log.iter().any(|line| line.contains(" rm ")),
        "dry-run must not remove images or volumes"
    );
}

#[test]
fn clean_requires_confirmation_when_stdin_is_not_a_tty() {
    let harness = Harness::new();
    harness.write_volumes(&volumes_fixture(&[UNUSED_VOLUME]));

    harness
        .agentbox_assert(&["clean"])
        .failure()
        .stderr(predicate::str::contains(
            "agentbox clean requires --yes or --dry-run when stdin or stderr is not a TTY",
        ));

    let log = harness.read_log();
    assert!(
        !log.iter().any(|line| line.contains(" rm ")),
        "non-TTY refusal must happen before deletion"
    );
}

#[test]
fn clean_confirmation_errors_are_stable() {
    assert_eq!(
        prompt::confirmation_error(inquire::InquireError::OperationInterrupted).to_string(),
        "confirmation interrupted",
    );
}

#[test]
fn clean_yes_removes_unused_default_images_and_cache_volumes() {
    let harness = Harness::new();
    let opencode_image = RuntimeKind::Opencode.default_image();
    let codex_image = RuntimeKind::Codex.default_image();
    harness.write_volumes(&volumes_fixture(&[UNUSED_VOLUME]));

    harness
        .agentbox_assert(&["clean", "--yes"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "removed image `{opencode_image}`"
        )))
        .stderr(predicate::str::contains(format!(
            "removed image `{codex_image}`"
        )))
        .stderr(predicate::str::contains(format!(
            "removed volume `{UNUSED_VOLUME}`"
        )));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("image ") && line.contains(&format!("args=rm {opencode_image}"))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("image ") && line.contains(&format!("args=rm {codex_image}"))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("volume ") && line.contains(&format!("args=rm {UNUSED_VOLUME}"))
    }));
}

#[test]
fn clean_skips_images_and_volumes_used_by_any_container() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let opencode_image = RuntimeKind::Opencode.default_image();
    let codex_image = RuntimeKind::Codex.default_image();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "used-id",
        USED_VOLUME,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "used-id",
        &managed_inspect_fixture(
            USED_VOLUME,
            workspace.canonical_git_root.as_str(),
            true,
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                USED_VOLUME,
            ),
        ),
    );
    harness.write_volumes(&volumes_fixture(&[USED_VOLUME]));

    harness
        .agentbox_assert(&["clean", "--yes"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "image `{opencode_image}`: used by container `{USED_VOLUME}`"
        )))
        .stderr(predicate::str::contains(format!(
            "volume `{USED_VOLUME}`: mounted by container `{USED_VOLUME}`"
        )))
        .stderr(predicate::str::contains(format!(
            "removed image `{codex_image}`"
        )));

    let log = harness.read_log();
    assert!(
        !log.iter().any(|line| {
            line.starts_with("image ") && line.contains(&format!("args=rm {opencode_image}"))
        }),
        "used image must not be removed"
    );
    assert!(
        !log.iter().any(|line| {
            line.starts_with("volume ") && line.contains(&format!("args=rm {USED_VOLUME}"))
        }),
        "mounted volume must not be removed"
    );
}

#[test]
fn clean_only_considers_agentbox_cache_volume_name_shape() {
    let harness = Harness::new();
    harness.write_volumes(&volumes_fixture(&[
        UNUSED_VOLUME,
        "agentbox-data",
        "agentbox-short-abc123",
        "other-agentbox-abcdef123456",
    ]));

    let output = harness.agentbox_output(&["clean", "--dry-run", "--volumes"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(stderr.contains(UNUSED_VOLUME));
    assert!(!stderr.contains("agentbox-data"));
    assert!(!stderr.contains("agentbox-short-abc123"));
    assert!(!stderr.contains("other-agentbox-abcdef123456"));
}

#[test]
fn clean_removes_runtime_image_state_after_image_delete() {
    let harness = Harness::new();
    let image = RuntimeKind::Opencode.default_image();
    let state_path = harness
        .state_home_path()
        .join("agentbox/runtime/opencode.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        runtime_state("opencode", "opencode-ai", &image),
    )
    .unwrap();

    harness
        .agentbox_assert(&["clean", "--yes", "--images"])
        .success();

    assert!(
        !state_path.exists(),
        "opencode runtime image state should be removed after image deletion"
    );
}

#[test]
fn clean_removes_unused_labeled_old_hash_images() {
    let harness = Harness::new();
    let old_image = "localhost/agentbox-opencode:ctx-0000000000000000";
    harness.write_images(&podman_images_fixture(&[runtime_image_fixture(
        RuntimeKind::Opencode,
        old_image,
        "0000000000000000",
    )]));

    harness
        .agentbox_assert(&["clean", "--yes", "--images"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "removed image `{old_image}`"
        )));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("image ") && line.contains(&format!("args=rm {old_image}"))
    }));
}

#[test]
fn clean_preserves_runtime_image_state_when_active_image_is_still_in_use() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let current_image = RuntimeKind::Opencode.default_image();
    let current_hash = default_image_context_hash();
    let old_image = "localhost/agentbox-opencode:ctx-0000000000000000";
    harness.write_images(&podman_images_fixture(&[
        runtime_image_fixture(RuntimeKind::Opencode, &current_image, current_hash),
        runtime_image_fixture(RuntimeKind::Opencode, old_image, "0000000000000000"),
    ]));
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        &workspace.container_name,
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &current_image,
            RuntimeKind::Opencode,
            49152,
        ),
    );

    let state_path = harness
        .state_home_path()
        .join("agentbox/runtime/opencode.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        runtime_state("opencode", "opencode-ai", &current_image),
    )
    .unwrap();

    harness
        .agentbox_assert(&["clean", "--yes", "--images"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "image `{current_image}`: used by container `{}`",
            workspace.container_name
        )))
        .stderr(predicate::str::contains(format!(
            "removed image `{old_image}`"
        )));

    assert!(
        state_path.exists(),
        "current runtime image state should be preserved while its image is in use"
    );
}

#[test]
fn clean_continues_after_delete_failures_and_exits_nonzero() {
    let harness = Harness::new();
    harness.write_volumes(&volumes_fixture(&[UNUSED_VOLUME]));
    harness.fail_operation("image-rm", "image is busy\n", 125);

    harness
        .agentbox_assert(&["clean", "--yes"])
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "removed volume `{UNUSED_VOLUME}`"
        )))
        .stderr(predicate::str::contains("partial clean failed"))
        .stderr(predicate::str::contains("image is busy"));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("volume ") && line.contains(&format!("args=rm {UNUSED_VOLUME}"))
    }));
}

fn runtime_state(runtime: &str, package: &str, image: &str) -> String {
    format!(
        r#"{{
  "runtime": "{runtime}",
  "package": "{package}",
  "install_source": "npm",
  "image": "{image}",
  "image_context_hash": "{}",
  "installed_version": "0.99.0",
  "latest_seen_version": "0.99.0",
  "latest_checked_at": 1,
  "image_built_at": 1
}}
"#,
        default_image_context_hash()
    )
}
