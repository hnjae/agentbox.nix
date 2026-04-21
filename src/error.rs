// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Cli(#[from] clap::Error),
    #[error("`{command}` is not implemented yet")]
    NotYetImplemented { command: &'static str },
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn not_yet_implemented(command: &'static str) -> Self {
        Self::NotYetImplemented { command }
    }

    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
