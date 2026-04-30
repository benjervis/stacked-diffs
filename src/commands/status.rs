use anyhow::Result;
use std::fmt::Write as FmtWrite;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git, git_silent, short, tip, Ctx};
use crate::errors::CmdResult;
use crate::output::{print_branch_tree, print_header, BranchRow, BranchTag, DIM, GREEN, RESET, YELLOW};

pub fn cmd_status(ctx: &Ctx, name: &str, remote: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;

    print_header(&format!("Stack: {name}  {DIM}remote: {remote}{RESET}"));
    println!();

    let current_branch = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();

    let all: Vec<String> = std::iter::once(stack.base.clone())
        .chain(stack.branches.clone())
        .collect();

    // Collect rows, computing detail lines (ahead/behind/remote) for each branch
    let mut rows: Vec<BranchRow<'_>> = Vec::with_capacity(all.len());
    let mut prev = stack.base.clone();

    for r#ref in &all {
        let tag = if current_branch.as_deref() == Some(r#ref.as_str()) {
            BranchTag::Current
        } else if !branch_exists(ctx, r#ref) {
            BranchTag::Missing
        } else {
            BranchTag::Normal
        };

        if !branch_exists(ctx, r#ref) {
            rows.push(BranchRow {
                name: r#ref,
                tag,
                detail: Some("missing locally".to_string()),
            });
            prev = r#ref.clone();
            continue;
        }

        let sha = tip(ctx, r#ref)?;
        let subject = git(ctx, &["log", "-1", "--format=%s", &sha])?;
        let short_sha = short(&sha);

        // Build detail string: commit + (for non-base) ahead/behind + remote info
        let mut detail = format!("{DIM}{short_sha}{RESET}  {subject}");

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

            let parent_color = if behind > 0 { YELLOW } else { GREEN };
            let mut parent_info = format!("{parent_color}+{ahead}{RESET}");
            if behind > 0 {
                write!(parent_info, " {YELLOW}-{behind} behind {prev} (rebase needed){RESET}").unwrap();
            } else {
                write!(parent_info, " {DIM}ahead of {prev}{RESET}").unwrap();
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
                    format!("{GREEN}✓ {remote} in sync{RESET}")
                } else {
                    format!("{YELLOW}↑{rahead} ↓{rbehind} vs {remote}{RESET}")
                }
            } else {
                format!("{DIM}no {remote} tracking ref{RESET}")
            };

            write!(detail, "\n     {parent_info}  {DIM}│{RESET}  {remote_info}").unwrap();
        }

        rows.push(BranchRow {
            name: r#ref,
            tag,
            detail: Some(detail),
        });
        prev = r#ref.clone();
    }

    print_branch_tree(&rows);
    Ok(Ok(()))
}
