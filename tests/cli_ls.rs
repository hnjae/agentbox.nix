use std::collections::BTreeMap;

use agentbox::cli::{Cli, Command, LsArgs, OutputFormat};
use agentbox::commands::ls::{render_json, render_table};
use agentbox::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::runtime::{AttachEndpoint, RuntimeKind};
use agentbox::session::{SessionMetadata, SessionRecord, SessionStatus};
use clap::Parser;

#[test]
fn ls_renders_all_status_rows_in_stable_order() {
    let sessions = vec![
        session("/workspace/b", "beta", SessionStatus::failed_unknown()),
        session("/workspace/a", "alpha-one", SessionStatus::Running),
        session("/workspace/c", "gamma", SessionStatus::Orphaned),
        session("/workspace/d", "delta", SessionStatus::failed_unknown()),
        session(
            "/workspace/a-duplicate",
            "alpha-two",
            SessionStatus::Duplicate,
        ),
    ];

    let table = render_table(&sessions);

    let alpha = line_index(&table, "/workspace/a");
    let alpha_dup = line_index(&table, "/workspace/a-duplicate");
    let beta = line_index(&table, "/workspace/b");
    let gamma = line_index(&table, "/workspace/c");
    let delta = line_index(&table, "/workspace/d");

    assert!(
        alpha < alpha_dup,
        "rows are sorted by root before container-name tie breaks"
    );
    assert!(
        alpha < beta && beta < gamma && gamma < delta,
        "roots sort lexicographically"
    );
    assert!(table.lines().next().unwrap().starts_with("ID"));
    assert!(table.contains("hash"));
    assert!(table.contains("running"));
    assert!(table.contains("orphaned"));
    assert!(table.contains("failed"));
    assert!(table.contains("duplicate"));
    assert!(table.contains("http://127.0.0.1:4096"));
    assert!(!table.contains("container name"));
    assert!(!table.contains("alpha-one"));
    assert!(table.ends_with('\n'));
    assert_no_box_drawing_borders(&table);
}

#[test]
fn ls_renders_unknown_for_unrecoverable_failed_fields() {
    let mut session = session(
        "/workspace/broken",
        "broken",
        SessionStatus::failed_unknown(),
    );
    session.metadata = SessionMetadata::from_labels(&BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
    ]));
    session.attach_endpoint = None;

    let table = render_table(&[session]);

    assert!(table.contains("unknown"));
    assert!(!table.contains("broken"));
}

#[test]
fn ls_renders_json_rows_in_stable_order() {
    let sessions = vec![
        session("/workspace/b", "beta", SessionStatus::failed_unknown()),
        session("/workspace/a", "alpha-one", SessionStatus::Running),
        session("/workspace/c", "gamma", SessionStatus::Orphaned),
        session("/workspace/a", "alpha-two", SessionStatus::Duplicate),
    ];

    let json = render_json(&sessions).unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    let names = rows
        .iter()
        .map(|row| row["container_name"].as_str().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(names, ["alpha-one", "alpha-two", "beta", "gamma"]);
    assert_eq!(rows[0]["id"], "hash");
    assert_eq!(rows[0]["canonical_git_root"], "/workspace/a");
    assert_eq!(rows[0]["runtime"], "opencode");
    assert_eq!(rows[0]["status"], "running");
    assert_eq!(rows[0]["endpoint"], "http://127.0.0.1:4096");
    assert_eq!(json.matches('\n').count(), 1);
}

#[test]
fn ls_renders_null_for_unrecoverable_json_fields() {
    let mut session = session(
        "/workspace/broken",
        "broken",
        SessionStatus::failed_unknown(),
    );
    session.metadata = SessionMetadata::from_labels(&BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
    ]));
    session.attach_endpoint = None;

    let json = render_json(&[session]).unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();

    assert_eq!(rows.len(), 1);
    assert!(rows[0]["id"].is_null());
    assert!(rows[0]["canonical_git_root"].is_null());
    assert!(rows[0]["runtime"].is_null());
    assert!(rows[0]["endpoint"].is_null());
    assert_eq!(rows[0]["status"], "failed");
    assert_eq!(rows[0]["container_name"], "broken");
}

#[test]
fn ls_rejects_legacy_json_flag() {
    let error = Cli::try_parse_from(["agentbox", "ls", "--json"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(error.to_string().contains("unexpected argument '--json'"));
}

#[test]
fn ls_defaults_to_table_output() {
    assert_eq!(
        Cli::try_parse_from(["agentbox", "ls"]).unwrap().command,
        Command::Ls(LsArgs {
            output: OutputFormat::Table,
        })
    );
}

fn session(root: &str, name: &str, status: SessionStatus) -> SessionRecord {
    SessionRecord {
        container_id: format!("{name}-id"),
        container_name: name.to_string(),
        metadata: metadata(root, name),
        runtime_kind: Some(RuntimeKind::Opencode),
        attach_endpoint: Some(AttachEndpoint {
            scheme: "http".to_string(),
            host_ip: "127.0.0.1".to_string(),
            host_port: 4096,
        }),
        container_running: status != SessionStatus::Failed(None),
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
        (LABEL_LAUNCH_DIRECTORY.to_string(), root.to_string()),
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

fn assert_no_box_drawing_borders(table: &str) {
    let border = table
        .chars()
        .find(|character| ('\u{2500}'..='\u{257f}').contains(character));
    assert!(border.is_none(), "table contains a border: {table}");
}
