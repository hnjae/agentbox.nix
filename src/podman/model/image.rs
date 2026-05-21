// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::de::{deserialize_map_or_null_default, deserialize_option_vec_or_string};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PodmanImage {
    #[serde(default, rename = "Repository", alias = "repository")]
    pub repository: String,
    #[serde(default, rename = "Tag", alias = "tag")]
    pub tag: String,
    #[serde(
        default,
        rename = "Names",
        alias = "names",
        deserialize_with = "deserialize_option_vec_or_string"
    )]
    pub names: Option<Vec<String>>,
    #[serde(
        default,
        rename = "Labels",
        alias = "labels",
        deserialize_with = "deserialize_map_or_null_default"
    )]
    pub labels: BTreeMap<String, String>,
}

impl PodmanImage {
    pub fn references(&self) -> Vec<String> {
        let mut references = self.names.clone().unwrap_or_default();

        if !self.repository.is_empty()
            && !self.tag.is_empty()
            && self.repository != "<none>"
            && self.tag != "<none>"
        {
            references.push(format!("{}:{}", self.repository, self.tag));
        }

        references.retain(|reference| !reference.is_empty() && reference != "<none>");
        references.sort();
        references.dedup();
        references
    }
}
