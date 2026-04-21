// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "agentbox")]
#[command(version)]
#[command(about = "Manage agentbox sessions", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Create and attach to a managed session.
    Run(RunArgs),
    /// Attach to an existing managed session.
    Attach(DirectoryArgs),
    /// List managed sessions.
    Ls,
    /// Remove a managed session.
    Rm(DirectoryArgs),
    /// Shell completion helpers.
    Completion,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RunArgs {
    /// Runtime image to persist on first session creation.
    #[arg(long)]
    pub image: Option<String>,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DirectoryArgs {
    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}
