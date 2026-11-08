//! The kinds of project Workbench knows, modelled so that a combination that
//! cannot exist (python with angular, typescript with uv) cannot be built.

use clap::{ValueEnum, builder::PossibleValue};

/// The `--language` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Language {
    Python,
    Typescript,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Typescript => "typescript",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonDependencyManager {
    Poetry,
    Uv,
}

impl PythonDependencyManager {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Poetry => "poetry",
            Self::Uv => "uv",
        }
    }

    /// The command prefix that launches tools inside the project's environment.
    pub fn activate_command(self) -> &'static str {
        match self {
            Self::Poetry => "poetry run",
            Self::Uv => "uv run",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypescriptDependencyManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl TypescriptDependencyManager {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }
}

/// The `--dependency-manager` flag: every manager, tagged with the language it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyManager {
    Python(PythonDependencyManager),
    Typescript(TypescriptDependencyManager),
}

impl DependencyManager {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Python(manager) => manager.as_str(),
            Self::Typescript(manager) => manager.as_str(),
        }
    }

    pub fn language(self) -> Language {
        match self {
            Self::Python(_) => Language::Python,
            Self::Typescript(_) => Language::Typescript,
        }
    }
}

impl ValueEnum for DependencyManager {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Python(PythonDependencyManager::Poetry),
            Self::Python(PythonDependencyManager::Uv),
            Self::Typescript(TypescriptDependencyManager::Npm),
            Self::Typescript(TypescriptDependencyManager::Pnpm),
            Self::Typescript(TypescriptDependencyManager::Yarn),
            Self::Typescript(TypescriptDependencyManager::Bun),
        ]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.as_str()))
    }
}

/// The `--framework` flag. Every framework here is a typescript one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TypescriptFramework {
    Angular,
}

impl TypescriptFramework {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Angular => "angular",
        }
    }
}

/// A project's language together with the options that only exist for that language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stack {
    Python {
        dependency_manager: PythonDependencyManager,
        venv: Option<String>,
    },
    Typescript {
        dependency_manager: TypescriptDependencyManager,
        framework: Option<TypescriptFramework>,
    },
}

impl Stack {
    pub fn language(&self) -> Language {
        match self {
            Self::Python { .. } => Language::Python,
            Self::Typescript { .. } => Language::Typescript,
        }
    }

    pub fn dependency_manager(&self) -> DependencyManager {
        match self {
            Self::Python {
                dependency_manager, ..
            } => DependencyManager::Python(*dependency_manager),
            Self::Typescript {
                dependency_manager, ..
            } => DependencyManager::Typescript(*dependency_manager),
        }
    }

    pub fn framework(&self) -> Option<TypescriptFramework> {
        match self {
            Self::Python { .. } => None,
            Self::Typescript { framework, .. } => *framework,
        }
    }

    pub fn venv(&self) -> Option<&str> {
        match self {
            Self::Python { venv, .. } => venv.as_deref(),
            Self::Typescript { .. } => None,
        }
    }

    /// The default command prefix for launching tools in the project, if the stack has one.
    pub fn activate_command(&self) -> Option<&'static str> {
        match self {
            Self::Python {
                dependency_manager, ..
            } => Some(dependency_manager.activate_command()),
            Self::Typescript { .. } => None,
        }
    }
}
