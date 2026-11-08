use crate::{cli::errors::CliErrors, mcp::errors::McpErrors};

pub type WorkbenchResult<T> = Result<T, WorkbenchErrors>;

#[derive(Debug)]
pub enum WorkbenchErrors {
    CliErrors(CliErrors),
    McpErrors(McpErrors),
}

impl std::fmt::Display for WorkbenchErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkbenchErrors::CliErrors(cli_errors) => cli_errors.fmt(f),
            WorkbenchErrors::McpErrors(mcp_errors) => mcp_errors.fmt(f),
        }
    }
}

impl std::error::Error for WorkbenchErrors {}

impl From<CliErrors> for WorkbenchErrors {
    fn from(value: CliErrors) -> Self {
        Self::CliErrors(value)
    }
}
