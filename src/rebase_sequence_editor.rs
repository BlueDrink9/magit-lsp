//! Transport for the initial interactive-rebase sequence editor.

use crate::Error;
use std::path::Path;

/// Environment variable carrying the external prepared-plan file to the helper.
pub const PLAN_FILE_ENV: &str = "MAGIT_LSP_REBASE_PLAN_FILE";

/// Copy the prepared plan bytes to Git's todo path without starting the server.
pub fn copy_plan(todo_path: &Path) -> Result<(), Error> {
    let source = std::env::var_os(PLAN_FILE_ENV)
        .ok_or_else(|| Error::new(format!("{PLAN_FILE_ENV} is not set")))?;
    let bytes = std::fs::read(&source)
        .map_err(|error| Error::new(format!("cannot read rebase plan {}: {error}", Path::new(&source).display())))?;
    std::fs::write(todo_path, bytes)
        .map_err(|error| Error::new(format!("cannot write rebase todo {}: {error}", todo_path.display())))
}

/// Return the executable's sequence-editor command, quoted for Git's shell.
pub fn command() -> Result<String, Error> {
    let exe = std::env::current_exe()
        .map_err(|error| Error::new(format!("cannot locate magit-lsp: {error}")))?;
    Ok(command_for(&exe))
}

/// Build a sequence-editor command for `exe`, quoted for `sh`.
pub fn command_for(exe: &Path) -> String {
    format!("{} --sequence-editor-helper", shell_quote(&exe.to_string_lossy()))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
