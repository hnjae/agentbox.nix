use std::collections::BTreeMap;

use agentbox::cli::{Cli, Command};
use agentbox::commands::ls::render_table;
use agentbox::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::session::{SessionMetadata, SessionRecord, SessionStatus};
use clap::Parser;

#[test]
fn ls_renders_all_status_rows_in_stable_order() {
    let sessions = vec![
        session("/workspace/b", "beta", SessionStatus::Failed),
        session("/workspace/a", "alpha-one", SessionStatus::Running),
        session("/workspace/c", "gamma", SessionStatus::Orphaned),
        session("/workspace/d", "delta", SessionStatus::Failed),
        session("/workspace/a", "alpha-two", SessionStatus::Duplicate),
    ];

    let table = render_table(&sessions);

    let alpha = line_index(&table, "alpha-one");
    let beta = line_index(&table, "beta");
    let gamma = line_index(&table, "gamma");
    let delta = line_index(&table, "delta");
    let alpha_dup = line_index(&table, "alpha-two");

    assert!(
        alpha < alpha_dup,
        "duplicate rows stay grouped by root then name"
    );
    assert!(
        alpha < beta && beta < gamma && gamma < delta,
        "roots sort lexicographically"
    );
    assert!(table.contains("running"));
    assert!(table.contains("orphaned"));
    assert!(table.contains("failed"));
    assert!(table.contains("duplicate"));
}

#[test]
fn ls_has_no_machine_readable_mode() {
    let error = Cli::try_parse_from(["agentbox", "ls", "--json"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(error.to_string().contains("unexpected argument '--json'"));
    assert!(matches!(
        Cli::try_parse_from(["agentbox", "ls"]).unwrap().command,
        Command::Ls
    ));
}

fn session(root: &str, name: &str, status: SessionStatus) -> SessionRecord {
    SessionRecord {
        container_id: format!("{name}-id"),
        container_name: name.to_string(),
        metadata: metadata(root, name),
        attach_endpoint: None,
        failure: None,
        status,
    }
}

fn metadata(root: &str, name: &str) -> SessionMetadata {
    SessionMetadata::from_labels(&BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
        (LABEL_GIT_ROOT.to_string(), root.to_string()),
        (LABEL_GIT_ROOT_HASH.to_string(), "hash".to_string()),
        (LABEL_RUNTIME.to_string(), "opencode".to_string()),
        (LABEL_IMAGE.to_string(), "image".to_string()),
        (LABEL_LOGICAL_NAME.to_string(), name.to_string()),
        (LABEL_ATTACH_SCHEME.to_string(), "http".to_string()),
        (LABEL_CONTAINER_PORT.to_string(), "4096".to_string()),
        (LABEL_CONTAINER_LISTEN_IP.to_string(), "0.0.0.0".to_string()),
    ]))
}

fn line_index(table: &str, needle: &str) -> usize {
    table
        .lines()
        .position(|line| line.contains(needle))
        .unwrap()
}
