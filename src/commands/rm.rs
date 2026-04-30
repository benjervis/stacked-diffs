use anyhow::Result;

use crate::config::{load_stack, remove_branch_from_config};
use crate::ctx::Ctx;
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, info, ok};

pub fn cmd_rm(ctx: &Ctx, name: &str, branch: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    if !stack.branches.contains(&branch.to_string()) {
        err_print(&format!("Branch '{branch}' is not in stack '{name}'."));
        return Ok(Err(CmdError::UserError));
    }

    if stack.branches.last().map(|s| s.as_str()) != Some(branch) {
        info(&format!("Note: '{branch}' is not at the top of the stack."));
        let idx = stack.branches.iter().position(|b| b == branch).unwrap();
        if idx + 1 < stack.branches.len() {
            info(&format!(
                "After removal, '{}' will still be based on '{branch}' until you rebase.",
                stack.branches[idx + 1]
            ));
        }
    }

    remove_branch_from_config(ctx, name, branch)?;
    ok(&format!("Removed '{branch}' from stack '{name}'."));
    info(&format!(
        "The local git branch '{branch}' was not deleted. Use 'git branch -D {branch}' if you also want to delete it."
    ));
    Ok(Ok(()))
}
