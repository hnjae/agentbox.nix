// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::de::deserialize_map_or_null_default;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanVolume {
    pub name: String,
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub mountpoint: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub options: BTreeMap<String, String>,
}
