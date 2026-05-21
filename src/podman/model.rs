// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod container;
mod de;
mod image;
mod network;
mod volume;

pub use container::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerMountKind,
    PodmanContainerState, PodmanHealth, PodmanHostConfig, PodmanNamespaces, PodmanPsContainer,
    PodmanPsPort,
};
pub(super) use de::parse_json;
pub use image::PodmanImage;
pub use network::{
    PodmanNetworkEndpoint, PodmanNetworkSettings, PodmanPortBinding, PodmanPublishedPort,
};
pub use volume::PodmanVolume;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn podman_ps_container_treats_null_labels_as_empty() {
        let containers: Vec<PodmanPsContainer> = serde_json::from_str(
            r#"[
  {
    "Id": "0123456789abcdef",
    "Image": "registry.example/image:latest",
    "Command": null,
    "Created": 1713681300,
    "CreatedAt": "2026-04-21 10:15:00 +0000 UTC",
    "Names": ["ambient-container"],
    "Ports": null,
    "Status": "Exited (0) 1 minute ago",
    "State": "exited",
    "Labels": null,
    "Mounts": null,
    "Networks": null,
    "Namespaces": null
  }
]"#,
        )
        .unwrap();

        assert_eq!(containers.len(), 1);
        assert!(containers[0].labels.is_empty());
    }
}
