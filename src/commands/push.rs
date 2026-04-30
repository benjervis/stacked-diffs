use anyhow::Result;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git_interactive, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, ok, step};

pub fn cmd_push(ctx: &Ctx, name: &str, remote: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    let count = stack.branches.len();
    if count == 0 {
        err_print(&format!("Stack '{name}' has no branches to push."));
        return Ok(Err(CmdError::UserError));
    }
    for (i, branch) in stack.branches.iter().enumerate() {
        if !branch_exists(ctx, branch) {
            err_print(&format!("Branch '{branch}' missing locally; aborting push."));
            return Ok(Err(CmdError::UserError));
        }
        step(&format!(
            "({}/{count}) Pushing '{branch}' to '{remote}' (--force-with-lease)...",
            i + 1
        ));
        if !git_interactive(ctx, &["push", "--force-with-lease", remote, branch])? {
            err_print(&format!("Push of '{branch}' failed."));
            return Ok(Err(CmdError::UserError));
        }
    }
    let plural = if count == 1 { "" } else { "es" };
    ok(&format!("Pushed {count} branch{plural} to '{remote}'."));
    Ok(Ok(()))
}
