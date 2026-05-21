// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::runtime::{RuntimeMount, RuntimeRunSpec};

use super::args::PodmanArgs;

pub(super) fn run_detached_args(
    container_name: &str,
    spec: &RuntimeRunSpec,
    host_gid: libc::gid_t,
) -> Vec<String> {
    let mut args = PodmanArgs::from(["run", "--detach", "--rm"]);
    append_common_run_args(&mut args, container_name, spec, host_gid);
    args.into_vec()
}

pub(super) fn run_foreground_args(
    container_name: &str,
    spec: &RuntimeRunSpec,
    host_gid: libc::gid_t,
    use_tty: bool,
) -> Vec<String> {
    let mut args = PodmanArgs::from(["run", "--rm", "--interactive"]);
    if use_tty {
        args.flag("--tty");
    }
    append_common_run_args(&mut args, container_name, spec, host_gid);
    args.into_vec()
}

pub(super) fn current_primary_gid() -> libc::gid_t {
    // SAFETY: getgid has no preconditions and only returns the current process real GID.
    unsafe { libc::getgid() }
}

fn append_common_run_args(
    args: &mut PodmanArgs,
    container_name: &str,
    spec: &RuntimeRunSpec,
    host_gid: libc::gid_t,
) {
    let gid = host_gid.to_string();
    args.option("--name", container_name);
    args.option("--userns", format!("keep-id:uid=1000,gid={gid}"));
    args.option("--user", format!("user:{gid}"));
    args.option("--group-add", "keep-groups");
    args.option("--workdir", spec.workdir().as_str());

    let create = spec.create();

    for (name, value) in create.labels() {
        args.key_value_option("--label", name, value);
    }

    for mount in create.mounts() {
        args.option("--mount", render_mount(mount));
    }

    for (name, value) in create.default_env() {
        args.key_value_option("--env", name, value);
    }

    if !create.network_enabled() {
        args.flag("--network=none");
    }

    for port in create.published_ports() {
        args.option("--publish", port);
    }

    args.flag(create.image());
    args.extend(create.command().iter().map(String::as_str));
}

fn render_mount(mount: &RuntimeMount) -> String {
    let mut options = vec![
        format!("type={}", mount.kind.podman_type()),
        format!("src={}", mount.source),
        format!("dst={}", mount.destination),
    ];
    if mount.read_only {
        options.push("ro".to_string());
    }
    if mount.kind == crate::runtime::RuntimeMountKind::Volume {
        options.push("U".to_string());
    }
    options.join(",")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::podman::args::strings;
    use crate::runtime::{RuntimeCreateSpec, RuntimeMount};

    #[test]
    fn run_detached_args_are_stable_and_complete() {
        let spec = RuntimeRunSpec::new(
            RuntimeCreateSpec::new(
                "localhost/agentbox-opencode:ctx-0123456789abcdef",
                BTreeMap::from([
                    ("io.agentbox.managed".to_string(), "true".to_string()),
                    ("io.agentbox.runtime".to_string(), "opencode".to_string()),
                ]),
                vec![
                    RuntimeMount::read_only_bind("/workspace", "/workspace"),
                    RuntimeMount::volume("agentbox-cache", "/home/user"),
                ],
                strings([
                    "opencode",
                    "serve",
                    "--hostname",
                    "0.0.0.0",
                    "--port",
                    "4096",
                ]),
                BTreeMap::from([("NIX_CONFIG".to_string(), "sandbox = false".to_string())]),
                false,
                vec!["127.0.0.1::4096".to_string()],
            ),
            "/workspace",
        );

        assert_eq!(
            run_detached_args("agentbox-demo", &spec, 1234),
            strings([
                "run",
                "--detach",
                "--rm",
                "--name",
                "agentbox-demo",
                "--userns",
                "keep-id:uid=1000,gid=1234",
                "--user",
                "user:1234",
                "--group-add",
                "keep-groups",
                "--workdir",
                "/workspace",
                "--label",
                "io.agentbox.managed=true",
                "--label",
                "io.agentbox.runtime=opencode",
                "--mount",
                "type=bind,src=/workspace,dst=/workspace,ro",
                "--mount",
                "type=volume,src=agentbox-cache,dst=/home/user,U",
                "--env",
                "NIX_CONFIG=sandbox = false",
                "--network=none",
                "--publish",
                "127.0.0.1::4096",
                "localhost/agentbox-opencode:ctx-0123456789abcdef",
                "opencode",
                "serve",
                "--hostname",
                "0.0.0.0",
                "--port",
                "4096",
            ])
        );
    }

    #[test]
    fn run_foreground_args_are_unmanaged_and_interactive() {
        let spec = RuntimeRunSpec::new(
            RuntimeCreateSpec::new(
                "localhost/agentbox-opencode:ctx-0123456789abcdef",
                BTreeMap::new(),
                vec![
                    RuntimeMount::read_only_bind("/workspace", "/workspace"),
                    RuntimeMount::volume("agentbox-cache", "/home/user"),
                ],
                strings(["opencode"]),
                BTreeMap::from([(
                    "OPENCODE_CONFIG_CONTENT".to_string(),
                    r#"{"autoupdate":false}"#.to_string(),
                )]),
                true,
                Vec::new(),
            ),
            "/workspace",
        );

        assert_eq!(
            run_foreground_args("agentbox-demo", &spec, 1234, true),
            strings([
                "run",
                "--rm",
                "--interactive",
                "--tty",
                "--name",
                "agentbox-demo",
                "--userns",
                "keep-id:uid=1000,gid=1234",
                "--user",
                "user:1234",
                "--group-add",
                "keep-groups",
                "--workdir",
                "/workspace",
                "--mount",
                "type=bind,src=/workspace,dst=/workspace,ro",
                "--mount",
                "type=volume,src=agentbox-cache,dst=/home/user,U",
                "--env",
                r#"OPENCODE_CONFIG_CONTENT={"autoupdate":false}"#,
                "localhost/agentbox-opencode:ctx-0123456789abcdef",
                "opencode",
            ])
        );
    }
}
