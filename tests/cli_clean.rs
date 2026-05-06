// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::runtime::default_image::{CODEX_DEFAULT_IMAGE, OPENCODE_DEFAULT_IMAGE};
use predicates::prelude::*;
use serde_json::json;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, managed_inspect_fixture, managed_ps_entry,
    opencode_managed_labels as managed_labels, operation_names, ps_fixture,
};

const UNUSED_VOLUME: &str = "agentbox-unused-abcdef123456";
const USED_VOLUME: &str = "agentbox-used-abcdef123456";

#[test]
fn clean_dry_run_prints_candidates_without_deleting() {
    let harness = Harness::new();
    harness.write_volumes(&volumes_fixture(&[UNUSED_VOLUME]));

    harness
        .agentbox_assert(&["clean", "--dry-run"])
        .success()
        .stdout(predicate::str::contains("cleanup candidates:"))
        .stdout(predicate::str::contains(OPENCODE_DEFAULT_IMAGE))
        .stdout(predicate::str::contains(CODEX_DEFAULT_IMAGE))
        .stdout(predicate::str::contains(UNUSED_VOLUME));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "image", "volume"]);
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
            "agentbox clean requires --yes or --dry-run when stdin is not a TTY",
        ));

    let log = harness.read_log();
    assert!(
        !log.iter().any(|line| line.contains(" rm ")),
        "non-TTY refusal must happen before deletion"
    );
}

#[test]
fn clean_yes_removes_unused_default_images_and_cache_volumes() {
    let harness = Harness::new();
    harness.write_volumes(&volumes_fixture(&[UNUSED_VOLUME]));

    harness
        .agentbox_assert(&["clean", "--yes"])
        .success()
        .stdout(predicate::str::contains(format!(
            "removed image `{OPENCODE_DEFAULT_IMAGE}`"
        )))
        .stdout(predicate::str::contains(format!(
            "removed image `{CODEX_DEFAULT_IMAGE}`"
        )))
        .stdout(predicate::str::contains(format!(
            "removed volume `{UNUSED_VOLUME}`"
        )));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("image ") && line.contains(&format!("args=rm {OPENCODE_DEFAULT_IMAGE}"))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("image ") && line.contains(&format!("args=rm {CODEX_DEFAULT_IMAGE}"))
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
        .stdout(predicate::str::contains(format!(
            "image `{OPENCODE_DEFAULT_IMAGE}`: used by container `{USED_VOLUME}`"
        )))
        .stdout(predicate::str::contains(format!(
            "volume `{USED_VOLUME}`: mounted by container `{USED_VOLUME}`"
        )))
        .stdout(predicate::str::contains(format!(
            "removed image `{CODEX_DEFAULT_IMAGE}`"
        )));

    let log = harness.read_log();
    assert!(
        !log.iter().any(|line| {
            line.starts_with("image ")
                && line.contains(&format!("args=rm {OPENCODE_DEFAULT_IMAGE}"))
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
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains(UNUSED_VOLUME));
    assert!(!stdout.contains("agentbox-data"));
    assert!(!stdout.contains("agentbox-short-abc123"));
    assert!(!stdout.contains("other-agentbox-abcdef123456"));
}

#[test]
fn clean_removes_runtime_image_state_after_image_delete() {
    let harness = Harness::new();
    let state_path = harness
        .state_home_path()
        .join("agentbox/runtime/opencode.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(&state_path, "{}\n").unwrap();

    harness
        .agentbox_assert(&["clean", "--yes", "--images"])
        .success();

    assert!(
        !state_path.exists(),
        "opencode runtime image state should be removed after image deletion"
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
        .stdout(predicate::str::contains(format!(
            "removed volume `{UNUSED_VOLUME}`"
        )))
        .stderr(predicate::str::contains("partial clean failed"))
        .stderr(predicate::str::contains("image is busy"));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("volume ") && line.contains(&format!("args=rm {UNUSED_VOLUME}"))
    }));
}

fn volumes_fixture(names: &[&str]) -> String {
    serde_json::to_string(
        &names
            .iter()
            .map(|name| {
                json!({
                    "Name": name,
                    "Driver": "local",
                    "Mountpoint": format!("/tmp/{name}"),
                    "Labels": {},
                    "Scope": "local",
                    "Options": {},
                })
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
