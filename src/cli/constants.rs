use crate::cli::commands::project::kinds::{PythonDependencyManager, TypescriptDependencyManager};

pub const BANNER: &str = r#"
 __        __         _    _                     _
 \ \      / /__  _ __| | _| |__   ___ _ __   ___| |__
  \ \ /\ / / _ \| '__| |/ / '_ \ / _ \ '_ \ / __| '_ \
   \ V  V / (_) | |  |   <| |_) |  __/ | | | (__| | | |
    \_/\_/ \___/|_|  |_|\_\_.__/ \___|_| |_|\___|_| |_|

                         W O R K B E N C H   C L I
"#;

pub const WORKBENCH_DIRECTORY: &str = ".workbench";
pub const WORKBENCH_CONFIG_FILE: &str = "config.yml";
pub const WORKBENCH_MEMORY_DIRECTORY: &str = "memory";
pub const WORKBENCH_INIT_MEMORY_FILE: &str = "INIT-MEMORY.md";
pub const PROJECT_ANNOTATIONS_FILE: &str = "annotations.yml";
pub const WORKBENCH_MCP_DIRECTORY: &str = "mcp";
pub const WORKBENCH_MCP_CONFIG_FILE: &str = "config.yml";
pub const WORKBENCH_CONTEXTS_DIRECTORY: &str = "contexts";
pub const WORKBENCH_LOGS_DIRECTORY: &str = "logs";
pub const WORKBENCH_SKILLS_DIRECTORY: &str = "skills";
pub const WORKBENCH_DEFAULT_CONTEXTS: [&str; 3] = ["production", "staging", "testing"];

pub const GITIGNORE_FILE: &str = ".gitignore";
pub const GITIGNORE_ENTRY: &str = ".workbench/";

pub const NAMESPACE_KEY: &str = "namespace:";
pub const PROJECT_ID_PLACEHOLDER: &str = "{id}";
pub const NAMESPACE_PLACEHOLDER: &str = "{namespace}";
pub const CONTEXT_NAME_PLACEHOLDER: &str = "{name}";
pub const DEFAULT_NAMESPACE_PREFIX: &str = "workspace";

pub const WORKBENCH_CONFIG_TEMPLATE: &str = include_str!("../../templates/init-config.yml");
pub const WORKBENCH_INIT_MEMORY_TEMPLATE: &str = include_str!("../../templates/init-memory.md");
pub const PROJECT_MEMORY_TEMPLATE: &str = include_str!("../../templates/project-memory.md");
pub const WORKBENCH_CONTEXT_TEMPLATE: &str = include_str!("../../templates/init-context.yml");
pub const WORKBENCH_MCP_CONFIG_TEMPLATE: &str = include_str!("../../templates/init-mcp-config.yml");

pub const SPINNER_TICK_MILLISECONDS: u64 = 100;

pub const PROJECTS_KEY: &str = "projects";
pub const DEFAULT_SEQUENCE_INDENT: &str = "  ";

pub const PYPROJECT_FILE: &str = "pyproject.toml";
pub const PYTHON_MARKERS: [&str; 3] = [PYPROJECT_FILE, "requirements.txt", "setup.py"];
pub const TYPESCRIPT_MARKERS: [&str; 3] = ["tsconfig.json", "package.json", ANGULAR_MARKER];
pub const ANGULAR_MARKER: &str = "angular.json";
pub const PYTHON_LOCKFILES: [(&str, PythonDependencyManager); 2] = [
    ("uv.lock", PythonDependencyManager::Uv),
    ("poetry.lock", PythonDependencyManager::Poetry),
];
pub const PYTHON_PYPROJECT_TABLES: [(&str, PythonDependencyManager); 2] = [
    ("[tool.uv", PythonDependencyManager::Uv),
    ("[tool.poetry", PythonDependencyManager::Poetry),
];
pub const TYPESCRIPT_LOCKFILES: [(&str, TypescriptDependencyManager); 5] = [
    ("pnpm-lock.yaml", TypescriptDependencyManager::Pnpm),
    ("yarn.lock", TypescriptDependencyManager::Yarn),
    ("bun.lock", TypescriptDependencyManager::Bun),
    ("bun.lockb", TypescriptDependencyManager::Bun),
    ("package-lock.json", TypescriptDependencyManager::Npm),
];
pub const DEFAULT_VENV: &str = ".venv";
pub const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

pub const LOG_FILE_PREFIX: &str = "workbench-";
pub const LOG_FILE_EXTENSION: &str = "log";

pub const STORAGE_KEY: &str = "storage";
pub const DEFAULT_STORAGE_HOST: &str = "localhost";
pub const DEFAULT_STORAGE_PORT: u16 = 7687;

pub const INDEX_LOCK_FILE: &str = "index.lock";
/// Files larger than this are skipped rather than parsed.
pub const MAX_INDEXED_FILE_BYTES: u64 = 1024 * 1024;
pub const PYTHON_EXTENSIONS: [&str; 2] = ["py", "pyi"];
pub const TYPESCRIPT_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];
/// Directories never indexed, in addition to every hidden directory (`.git`, `.venv`, ...).
pub const SKIPPED_DIRECTORIES: [&str; 7] = [
    "node_modules",
    "__pycache__",
    "venv",
    "dist",
    "build",
    "coverage",
    "target",
];
