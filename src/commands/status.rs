use anyhow::Result;
use std::fmt::Write as FmtWrite;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git, git_silent, short, tip, Ctx};
use crate::errors::CmdResult;

pub fn cmd_status(ctx: &Ctx, name: &str, remote: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    println!("Stack: {name} (remote: {remote})");
    let mut prev = stack.base.clone();
    let all: Vec<String> = std::iter::once(stack.base.clone())
        .chain(stack.branches.clone())
        .collect();

    for r#ref in &all {
        if !branch_exists(ctx, r#ref) {
            println!("  {ref}  — missing locally");
            continue;
        }
        let sha = tip(ctx, r#ref)?;
        let subject = git(ctx, &["log", "-1", "--format=%s", &sha])?;
        println!("  {ref}  {}  {subject}", short(&sha));

        if r#ref != &stack.base {
            let counts = git(
                ctx,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("{prev}...{ref}"),
                ],
            )?;
            let parts: Vec<&str> = counts.split_whitespace().collect();
            let behind: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let ahead: u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let mut parent_info = format!("ahead {ahead} of {prev}");
            if behind > 0 {
                write!(parent_info, ", behind {behind} (rebase needed)").unwrap();
            }

            let remote_info = if git_silent(
                ctx,
                &["rev-parse", "--verify", &format!("{remote}/{ref}")],
            ) {
                let rcounts = git(
                    ctx,
                    &[
                        "rev-list",
                        "--left-right",
                        "--count",
                        &format!("{remote}/{ref}...{ref}"),
                    ],
                )?;
                let rparts: Vec<&str> = rcounts.split_whitespace().collect();
                let rbehind: u64 = rparts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                let rahead: u64 = rparts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                if rahead == 0 && rbehind == 0 {
                    format!("{remote} in sync")
                } else {
                    format!("{remote}: ahead {rahead}, behind {rbehind}")
                }
            } else {
                format!("no {remote} tracking ref")
            };

            println!("      {parent_info} | {remote_info}");
        }
        prev = r#ref.clone();
    }
    Ok(Ok(()))
}
