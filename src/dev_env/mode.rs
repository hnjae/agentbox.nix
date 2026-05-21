// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum DevEnvMode {
    Auto,
    None,
}

impl DevEnvMode {
    fn variants() -> &'static [Self] {
        <Self as ValueEnum>::value_variants()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }

    pub fn supported_values() -> Vec<&'static str> {
        Self::variants().iter().map(|mode| mode.as_str()).collect()
    }
}

impl fmt::Display for DevEnvMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
