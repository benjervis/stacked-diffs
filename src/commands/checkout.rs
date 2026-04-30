use anyhow::Result;
use std::io;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git, git_ok, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, ok};

pub fn cmd_checkout(ctx: &Ctx, name: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    let all: Vec<String> = std::iter::once(stack.base.clone())
        .chain(stack.branches)
        .collect();

    if all.is_empty() {
        err_print(&format!("Stack '{name}' has no branches to checkout."));
        return Ok(Err(CmdError::UserError));
    }

    // Show the list with numbers.
    println!("Stack: {name}");
    println!("Select a branch to checkout:");
    for (i, branch) in all.iter().enumerate() {
        let current = if git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).as_deref().ok() == Some(branch.as_str()) {
            " (current)"
        } else if !branch_exists(ctx, branch) {
            " (missing)"
        } else {
            ""
        };
        println!("  {}. {}{}", i + 1, branch, current);
    }
    println!();

    // Prompt for a number.
    loop {
        eprint!("Enter number (1-{}): ", all.len());
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            err_print("Failed to read input.");
            return Ok(Err(CmdError::UserError));
        }
        match input.trim().parse::<usize>() {
            Ok(n) if 1 <= n && n <= all.len() => {
                let branch = &all[n - 1];
                if !branch_exists(ctx, branch) {
                    err_print(&format!("Branch '{}' does not exist locally.", branch));
                    return Ok(Err(CmdError::UserError));
                }
                if git_ok(ctx, &["checkout", branch])? {
                    ok(&format!("Checked out '{}'.", branch));
                    return Ok(Ok(()));
                } else {
                    err_print(&format!("Failed to checkout '{}'.", branch));
                    return Ok(Err(CmdError::UserError));
                }
            }
            Ok(_) => {
                err_print(&format!("Enter a number between 1 and {}.", all.len()));
                continue;
            }
            Err(_) => {
                err_print("Invalid input. Enter a number.");
                continue;
            }
        }
    }
}