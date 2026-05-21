// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use crate::error::Result;

use super::installed_command;

pub(super) fn generate_stdout() -> Result<()> {
    let command = installed_command::command();
    let mut stdout = std::io::stdout().lock();

    clap_mangen::Man::new(command).render(&mut stdout)?;
    Ok(())
}

pub(super) fn generate_all_to(directory: &Path) -> Result<()> {
    let command = installed_command::command();

    clap_mangen::generate_to(command, directory)?;
    Ok(())
}
