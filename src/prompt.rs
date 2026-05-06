// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fmt::Display;
use std::io::{self, IsTerminal};

use inquire::{Confirm, InquireError, MultiSelect, Select};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice<T> {
    label: String,
    value: T,
}

impl<T> Choice<T> {
    pub fn new(label: impl Into<String>, value: T) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> Display for Choice<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

pub(crate) fn sort_choices_by_label<T>(choices: &mut [Choice<T>]) {
    choices.sort_by(|left, right| left.label.cmp(&right.label));
}

pub fn select_one<T>(
    message: &'static str,
    options: Vec<T>,
    non_tty_error: &'static str,
) -> Result<T>
where
    T: Display,
{
    require_interactive_terminal(non_tty_error)?;
    Select::new(message, options)
        .prompt()
        .map_err(selection_error)
}

pub fn select_many<T>(
    message: &'static str,
    options: Vec<T>,
    non_tty_error: &'static str,
) -> Result<Vec<T>>
where
    T: Display,
{
    require_interactive_terminal(non_tty_error)?;
    MultiSelect::new(message, options)
        .prompt()
        .map_err(selection_error)
}

pub fn confirm(message: &'static str, default: bool, non_tty_error: &'static str) -> Result<bool> {
    require_interactive_terminal(non_tty_error)?;
    Confirm::new(message)
        .with_default(default)
        .prompt_skippable()
        .map(|answer| answer.unwrap_or(false))
        .map_err(confirmation_error)
}

pub fn require_interactive_terminal(non_tty_error: &'static str) -> Result<()> {
    if io::stdin().is_terminal() && io::stderr().is_terminal() {
        Ok(())
    } else {
        Err(Error::msg(non_tty_error))
    }
}

pub fn confirmation_error(error: InquireError) -> Error {
    match error {
        InquireError::OperationInterrupted => Error::msg("confirmation interrupted"),
        InquireError::NotTTY => Error::msg("interactive confirmation requires a TTY"),
        error => Error::msg(format!("interactive confirmation failed: {error}")),
    }
}

pub fn selection_error(error: InquireError) -> Error {
    match error {
        InquireError::OperationCanceled => Error::msg("selection canceled"),
        InquireError::OperationInterrupted => Error::msg("selection interrupted"),
        InquireError::NotTTY => Error::msg("interactive selection requires a TTY"),
        error => Error::msg(format!("interactive selection failed: {error}")),
    }
}
