use std::{env, fs, path::PathBuf, process};

use super::Workspace;
use crate::cli::errors::{CliErrors, CliResult};

/// A scratch workspace whose directory is removed when dropped.
struct ScratchWorkspace(PathBuf);

impl Drop for ScratchWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch_workspace(name: &str, config: &str) -> CliResult<(ScratchWorkspace, Workspace)> {
    let root = env::temp_dir().join(format!("workbench-workspace-{name}-{}", process::id()));
    fs::create_dir_all(root.join(".workbench"))?;
    fs::write(root.join(".workbench/config.yml"), config)?;
    let workspace = Workspace::find(&root)?;
    Ok((ScratchWorkspace(root), workspace))
}

#[test]
fn storage_reads_username_and_password_variable() -> CliResult<()> {
    let (_scratch, workspace) = scratch_workspace(
        "storage-auth",
        "version: 1\nstorage:\n  host: db\n  port: 7688\n  namespace: ns\n  username: wb\n  password_env: MG_PASS\nprojects: []\n",
    )?;
    let settings = workspace.storage()?;
    assert_eq!(
        (
            settings.host.as_str(),
            settings.port,
            settings.namespace.as_deref()
        ),
        ("db", 7688, Some("ns"))
    );
    assert_eq!(settings.username.as_deref(), Some("wb"));
    assert_eq!(
        settings.password(|name| (name == "MG_PASS").then(|| "secret".to_owned()))?,
        Some("secret".to_owned())
    );
    assert!(matches!(
        settings.password(|_| None),
        Err(CliErrors::MissingPasswordVariable(name)) if name == "MG_PASS"
    ));
    Ok(())
}

#[test]
fn storage_without_a_password_variable_has_no_password() -> CliResult<()> {
    let (_scratch, workspace) = scratch_workspace("storage-plain", "version: 1\nprojects: []\n")?;
    let settings = workspace.storage()?;
    assert_eq!(
        (
            settings.host.as_str(),
            settings.port,
            settings.namespace.as_deref()
        ),
        ("localhost", 7687, None)
    );
    assert_eq!(settings.password(|_| Some("ignored".to_owned()))?, None);
    Ok(())
}

#[test]
fn connection_errors_keep_details_out_of_the_message() {
    let error = CliErrors::MemgraphConnection {
        address: "localhost:7687".to_owned(),
        details: "Connection refused".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "Could not connect to Memgraph at localhost:7687. Is it running?"
    );
    assert!(format!("{error:?}").contains("Connection refused"));
}

#[test]
fn storage_errors_say_what_to_fix() {
    assert_eq!(
        CliErrors::MissingStorageNamespace(PathBuf::from(".workbench/config.yml")).to_string(),
        "The workspace config .workbench/config.yml has no `storage.namespace`. Add one under `storage`."
    );
    assert_eq!(
        CliErrors::MissingPasswordVariable("MG_PASS".to_owned()).to_string(),
        "`storage.password_env` names the environment variable `MG_PASS`, which is not set."
    );
    assert_eq!(
        CliErrors::ProjectCleanupFailed("boom".to_owned()).to_string(),
        "Could not remove the indexed data of projects no longer in the config."
    );
}
