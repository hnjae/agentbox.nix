// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use comfy_table::{Table, presets::NOTHING};
use serde::Serialize;

use crate::error::Result;

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
