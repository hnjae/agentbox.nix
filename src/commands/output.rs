// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use clap::ValueEnum;
use comfy_table::{Table, presets::NOTHING};
use serde::Serialize;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

impl OutputFormat {
    fn variants() -> &'static [Self] {
        <Self as ValueEnum>::value_variants()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Json => "json",
        }
    }

    pub fn supported_values() -> Vec<&'static str> {
        Self::variants()
            .iter()
            .map(|format| format.as_str())
            .collect()
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub(super) fn table(headers: impl Into<comfy_table::Row>) -> Table {
    let mut table = Table::new();
    table.load_preset(NOTHING);
    table.set_header(headers);
    table
}

pub(super) fn render_table(mut table: Table) -> String {
    trim_outer_padding(&mut table);
    format!("{}\n", table.trim_fmt())
}

pub(super) fn render_json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string(value)?))
}

fn trim_outer_padding(table: &mut Table) {
    let column_count = table.column_count();
    if column_count == 0 {
        return;
    }

    let last_column = column_count - 1;
    for (index, column) in table.column_iter_mut().enumerate() {
        let left = if index == 0 { 0 } else { 1 };
        let right = if index == last_column { 0 } else { 1 };
        column.set_padding((left, right));
    }
}
