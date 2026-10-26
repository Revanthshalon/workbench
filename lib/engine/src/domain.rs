use std::fmt;

/// A coding agent platform Workbench can be configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// GitHub Copilot CLI.
    CopilotCli,
    /// Claude Code.
    ClaudeCode,
    /// OpenAI Codex.
    Codex,
}

impl Platform {
    /// Platforms configured when none are named.
    pub const DEFAULTS: [Self; 2] = [Self::CopilotCli, Self::ClaudeCode];
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CopilotCli => "Copilot CLI",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        })
    }
}
