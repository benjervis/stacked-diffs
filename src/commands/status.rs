use anyhow::Result;

use crate::commands::rebase::fetch_and_fast_forward;
use crate::config::load_stack;
use crate::ctx::{branch_exists, git, git_silent, short, tip, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{
    err_print, info, print_branch_tree, print_header, step, BranchRow, BranchTag, BOLD, CYAN, DIM, GREEN, RED, RESET, YELLOW,
};

pub fn cmd_status(ctx: &Ctx, name: &str, remote: &str, check: bool) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;

    // If --check flag is used, fetch all stack branches and detect if rebasing is needed
    if check {
        step(&format!("Fetching stack branches from {remote}..."));
        
        // Check if remote exists before trying to fetch
        if git_silent(ctx, &["remote", "get-url", remote]) {
            // Fetch all stack branches including base
            let all_refs: Vec<String> = std::iter::once(stack.base.clone())
                .chain(stack.branches.clone())
                .collect();
            
            for r#ref in &all_refs {
                git_silent(ctx, &["fetch", remote, r#ref]);
            }
            
            // Fast-forward the base branch if needed
            if let Err(e) = fetch_and_fast_forward(ctx, &stack.base, remote) {
                err_print(&format!("Failed to fast-forward base branch '{}': {}", stack.base, e));
                return Ok(Err(CmdError::GitError));
            }
            
            info(&format!("Fetched {} branches from {remote}", all_refs.len()));
        } else {
            info(&format!("Remote '{remote}' not found - skipping fetch"));
        }
        println!();
    }

    print_header(name, Some(&format!("remote: {remote}")));
    println!();

    let current_branch = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();

    let all: Vec<String> = std::iter::once(stack.base.clone())
        .chain(stack.branches.clone())
        .collect();

    // Collect rows, computing detail lines (ahead/behind/remote) for each branch
    let mut rows: Vec<BranchRow<'_>> = Vec::with_capacity(all.len());
    let mut prev = stack.base.clone();
    let mut needs_rebase = false;

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
                detail: vec![format!("{DIM}missing locally{RESET}")],
            });
            prev = r#ref.clone();
            continue;
        }

        let sha = tip(ctx, r#ref)?;
        let subject = git(ctx, &["log", "-1", "--format=%s", &sha])?;
        let short_sha = short(&sha);

        // Line 1: short SHA + commit subject
        let commit_line = format!("{DIM}{short_sha}{RESET}  {subject}");

        let mut detail = vec![commit_line];

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

            // Track if this branch needs rebasing
            if behind > 0 {
                needs_rebase = true;
            }

            let parent_str = if behind > 0 {
                format!(
                    "{YELLOW}+{ahead}  -{behind} behind {prev} (rebase needed){RESET}"
                )
            } else {
                format!("{GREEN}+{ahead}{RESET}  {DIM}ahead of {prev}{RESET}")
            };

            let remote_str = if git_silent(
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
                
                // Track if branch is behind remote (remote has commits local doesn't, or diverged)
                if rbehind > 0 {
                    needs_rebase = true;
                }
                
                if rahead == 0 && rbehind == 0 {
                    format!("{GREEN}✓ {remote} in sync{RESET}")
                } else {
                    format!("{YELLOW}↑{rahead} ↓{rbehind} vs {remote}{RESET}")
                }
            } else {
                format!("{DIM}no {remote} tracking ref{RESET}")
            };

            // Line 2: ahead/behind parent  |  remote sync
            detail.push(format!("{parent_str}  {DIM}│{RESET}  {remote_str}"));
        }

        rows.push(BranchRow {
            name: r#ref,
            tag,
            detail,
        });
        prev = r#ref.clone();
    }

    print_branch_tree(&rows);
    
    // In check mode, print summary and return appropriate exit code
    if check {
        println!();
        if needs_rebase {
            err_print(&format!("{RED}✗{RESET} {BOLD}Rebase required{RESET} - one or more branches are behind their parent or have diverged from remote"));
            info(&format!("Run '{CYAN}sd rebase {name}{RESET}' to update the stack"));
            return Ok(Err(CmdError::NeedsRebase));
        } else {
            info(&format!("{GREEN}✓{RESET} {BOLD}Stack is up to date{RESET} - no rebasing needed"));
            return Ok(Ok(()));
        }
    }
    
    Ok(Ok(()))
}
