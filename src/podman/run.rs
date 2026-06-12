// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::runtime::{RuntimeMount, RuntimeMountKind, RuntimeRunSpec};

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

    if let Some(cpus) = &create.resource_limits().cpus
        && !cpus.is_unlimited()
    {
        args.option("--cpus", cpus.to_string());
    }

    if let Some(memory) = &create.resource_limits().memory
        && !memory.is_unlimited()
    {
        args.option("--memory", memory.to_string());
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
        format!("type={}", podman_mount_type(mount.kind())),
        format!("src={}", mount.source()),
        format!("dst={}", mount.destination()),
    ];
    if mount.is_read_only() {
        options.push("ro".to_string());
    }
    if mount.is_volume() {
        options.push("U".to_string());
    }
    options.join(",")
}

fn podman_mount_type(kind: RuntimeMountKind) -> &'static str {
    match kind {
        RuntimeMountKind::Bind => "bind",
        RuntimeMountKind::Volume => "volume",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::ResourceLimits;
    use crate::podman::args::strings;
    use crate::runtime::{RuntimeCreateSpec, RuntimeMount};

    #[test]
    fn run_detached_args_are_stable_and_complete() {
        let spec = RuntimeRunSpec::new(
            RuntimeCreateSpec::builder("localhost/agentbox-opencode:ctx-0123456789abcdef")
                .labels(BTreeMap::from([
                    ("io.agentbox.managed".to_string(), "true".to_string()),
                    ("io.agentbox.runtime".to_string(), "opencode".to_string()),
                ]))
                .mounts(vec![
                    RuntimeMount::read_only_bind("/workspace", "/workspace"),
                    RuntimeMount::volume("agentbox-cache", "/home/user"),
                ])
                .command(strings([
                    "opencode",
                    "serve",
                    "--hostname",
                    "0.0.0.0",
                    "--port",
                    "4096",
                ]))
                .default_env(BTreeMap::from([(
                    "NIX_CONFIG".to_string(),
                    "sandbox = false".to_string(),
                )]))
                .resource_limits(ResourceLimits {
                    cpus: Some("1.5".parse().unwrap()),
                    memory: Some("512m".parse().unwrap()),
                })
                .network_enabled(false)
                .published_ports(vec!["127.0.0.1::4096".to_string()])
                .build(),
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
                "--cpus",
                "1.5",
                "--memory",
                "512m",
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
            RuntimeCreateSpec::builder("localhost/agentbox-opencode:ctx-0123456789abcdef")
                .mounts(vec![
                    RuntimeMount::read_only_bind("/workspace", "/workspace"),
                    RuntimeMount::volume("agentbox-cache", "/home/user"),
                ])
                .command(strings(["opencode"]))
                .default_env(BTreeMap::from([(
                    "OPENCODE_CONFIG_CONTENT".to_string(),
                    r#"{"autoupdate":false}"#.to_string(),
                )]))
                .build(),
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

    #[test]
    fn zero_resource_limits_are_not_rendered() {
        let spec = RuntimeRunSpec::new(
            RuntimeCreateSpec::builder("localhost/agentbox-opencode:ctx-0123456789abcdef")
                .resource_limits(ResourceLimits {
                    cpus: Some("0".parse().unwrap()),
                    memory: Some("0".parse().unwrap()),
                })
                .command(strings(["opencode"]))
                .build(),
            "/workspace",
        );

        let args = run_detached_args("agentbox-demo", &spec, 1234);

        assert!(!args.contains(&"--cpus".to_string()));
        assert!(!args.contains(&"--memory".to_string()));
    }
}
