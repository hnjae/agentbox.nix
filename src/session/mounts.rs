// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::podman::PodmanContainerMount;

pub(super) fn has_volume_mount_destination(
    mounts: &[PodmanContainerMount],
    destination: &str,
) -> bool {
    mounts
        .iter()
        .any(|mount| mount.kind.is_volume() && mount.destination == destination)
}
