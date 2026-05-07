// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::runtime::{RuntimeMount, RuntimeRunSpec};

use super::args::PodmanArgs;

pub(super) fn run_detached_args(
    container_name: &str,
    spec: &RuntimeRunSpec,
    host_gid: libc::gid_t,
) -> Vec<String> {
    let mut args = PodmanArgs::from(["run", "--detach", "--rm"]);
    let gid = host_gid.to_string();
    args.option("--name", container_name);
    args.option("--userns", format!("keep-id:uid=1000,gid={gid}"));
    args.option("--user", format!("user:{gid}"));
    args.option("--group-add", "keep-groups");
    args.option("--workdir", spec.workdir().as_str());

    let create = spec.create();

    for (name, value) in &create.labels {
        args.key_value_option("--label", name, value);
    }

    for mount in &create.mounts {
        args.option("--mount", render_mount(mount));
    }

    for (name, value) in &create.default_env {
        args.key_value_option("--env", name, value);
    }

    if !create.network_enabled {
        args.flag("--network=none");
    }

    for port in &create.published_ports {
        args.option("--publish", port);
    }

    args.flag(create.image.as_str());
    args.extend(create.command.iter().map(String::as_str));
    args.into_vec()
}

pub(super) fn current_primary_gid() -> libc::gid_t {
    // SAFETY: getgid has no preconditions and only returns the current process real GID.
    unsafe { libc::getgid() }
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
            RuntimeCreateSpec {
                image: "localhost/agentbox-opencode:ctx-0123456789abcdef".to_string(),
                labels: BTreeMap::from([
                    ("io.agentbox.managed".to_string(), "true".to_string()),
                    ("io.agentbox.runtime".to_string(), "opencode".to_string()),
                ]),
                mounts: vec![
                    RuntimeMount::read_only_bind("/workspace", "/workspace"),
                    RuntimeMount::volume("agentbox-cache", "/home/user"),
                ],
                command: strings([
                    "opencode",
                    "serve",
                    "--hostname",
                    "0.0.0.0",
                    "--port",
                    "4096",
                ]),
                default_env: BTreeMap::from([(
                    "NIX_CONFIG".to_string(),
                    "sandbox = false".to_string(),
                )]),
                network_enabled: false,
                published_ports: vec!["127.0.0.1::4096".to_string()],
            },
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
}
