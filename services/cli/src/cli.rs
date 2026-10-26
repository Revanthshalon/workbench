//! Driving adapter: command-line arguments.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::utils::constants::BANNER;

/// Set up and manage Workbench for your coding agent.
#[derive(Debug, Parser)]
#[command(version, about, before_help = BANNER, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Configure Workbench for coding agent platforms.
    Init {
        /// Platforms to configure, comma-separated [default: copilot-cli,claude-code].
        #[arg(long, value_enum, value_delimiter = ',')]
        platform: Vec<PlatformArg>,
        /// Directory to create `.workbench-cli/` in.
        #[arg(long, value_name = "DIR", default_value = ".")]
        out: PathBuf,
    },
}

/// CLI spelling of [`engine::Platform`].
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PlatformArg {
    /// GitHub Copilot CLI.
    CopilotCli,
    /// Claude Code.
    ClaudeCode,
    /// OpenAI Codex.
    Codex,
}

impl From<PlatformArg> for engine::Platform {
    fn from(p: PlatformArg) -> Self {
        match p {
            PlatformArg::CopilotCli => Self::CopilotCli,
            PlatformArg::ClaudeCode => Self::ClaudeCode,
            PlatformArg::Codex => Self::Codex,
        }
    }
}
