use anyhow::Result;
use std::fs;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git_interactive, git_ok, repo_clean, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, ok};

pub fn cmd_add(ctx: &Ctx, name: &str, branch: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    if branch == stack.base {
        err_print(&format!("Branch '{branch}' is the base of stack '{name}'."));
        return Ok(Err(CmdError::UserError));
    }
    if stack.branches.contains(&branch.to_string()) {
        err_print(&format!("Branch '{branch}' is already in stack '{name}'."));
        return Ok(Err(CmdError::UserError));
    }
    if branch_exists(ctx, branch) {
        err_print(&format!(
            "Branch '{branch}' already exists locally. Choose a different name or delete it first."
        ));
        return Ok(Err(CmdError::UserError));
    }
    if !repo_clean(ctx)? {
        err_print("Working tree is dirty. Commit or stash changes first.");
        git_interactive(ctx, &["status", "--short", "--untracked-files=no"])?;
        return Ok(Err(CmdError::UserError));
    }

    let top = stack.branches.last().unwrap_or(&stack.base).clone();
    if !git_ok(ctx, &["checkout", &top])? {
        err_print(&format!("Couldn't checkout '{top}'."));
        return Ok(Err(CmdError::UserError));
    }
    if !git_ok(ctx, &["checkout", "-b", branch])? {
        err_print(&format!("Couldn't create branch '{branch}' from '{top}'."));
        return Ok(Err(CmdError::UserError));
    }

    let file = ctx.stacks_dir.join(name);
    let mut content = fs::read_to_string(&file)?;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(branch);
    content.push('\n');
    fs::write(&file, content)?;

    ok(&format!("Created '{branch}' from '{top}' and added to stack '{name}'."));
    Ok(Ok(()))
}
