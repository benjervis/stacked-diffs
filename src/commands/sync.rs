use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

use crate::commands::rebase::{do_rebase, fetch_and_fast_forward};
use crate::config::{load_stack, remove_branch_from_config};
use crate::ctx::{branch_exists, git, git_silent, repo_clean, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, info, ok, step};

#[derive(Debug, PartialEq)]
pub enum PrState {
    Merged,
    Closed,
    Open,
    NoPr,
}

/// Check that `gh` is installed and authenticated, bailing with a clear message if not.
pub fn check_gh() -> Result<()> {
    let installed = Command::new("gh")
        .args(["--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !installed {
        bail!("'gh' (GitHub CLI) is not installed. Install it from https://cli.github.com/ and run 'gh auth login'.");
    }

    let authed = Command::new("gh")
        .args(["auth", "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !authed {
        bail!("'gh' is installed but not authenticated. Run 'gh auth login' first.");
    }
    Ok(())
}

pub fn gh_pr_state(branch: &str) -> Result<PrState> {
    let out = Command::new("gh")
        .args(["pr", "view", branch, "--json", "state", "--jq", ".state"])
        .output()
        .context("failed to run gh")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("no pull requests found") {
            return Ok(PrState::NoPr);
        }
        bail!("gh pr view failed for '{}': {}", branch, stderr.trim());
    }

    let state = String::from_utf8(out.stdout)?.trim().to_uppercase();
    Ok(match state.as_str() {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Open,
    })
}

pub fn cmd_sync(ctx: &Ctx, name: &str, remote: &str) -> Result<CmdResult> {
    // check_gh errors are user-facing; let main's error handler print them.
    check_gh()?;

    if !repo_clean(ctx)? {
        err_print("Working tree is dirty. Commit or stash changes before syncing.");
        crate::ctx::git_interactive(ctx, &["status", "--short"])?;
        return Ok(Err(CmdError::UserError));
    }

    let stack = load_stack(ctx, name)?;
    let base = &stack.base;
    let branches = &stack.branches;

    if branches.is_empty() {
        info(&format!("Stack '{name}' has no branches — nothing to sync."));
        return Ok(Ok(()));
    }

    // Fetch and fast-forward the base so the subsequent rebase has an up-to-date target.
    fetch_and_fast_forward(ctx, base, remote)?;

    // Check PR state for every branch.
    step("Checking PR states via gh...");
    let mut states: Vec<(&str, PrState)> = Vec::new();
    for branch in branches {
        let state = gh_pr_state(branch)?;
        let label = match &state {
            PrState::Merged => "merged",
            PrState::Closed => "closed (not merged)",
            PrState::Open => "open",
            PrState::NoPr => "no PR",
        };
        info(&format!("  {branch}: {label}"));
        states.push((branch.as_str(), state));
    }

    // Abort immediately if any PR is closed-but-not-merged.
    let closed: Vec<&str> = states
        .iter()
        .filter(|(_, s)| *s == PrState::Closed)
        .map(|(b, _)| *b)
        .collect();
    if !closed.is_empty() {
        eprintln!();
        err_print(&format!(
            "Aborting: {} branch{} {} a closed (unmerged) PR:",
            closed.len(),
            if closed.len() == 1 { "" } else { "es" },
            if closed.len() == 1 { "has" } else { "have" },
        ));
        for b in &closed {
            err_print(&format!("  {b}"));
        }
        info("These branches were closed without merging. The stack assumptions are broken.");
        info("Resolve manually (remove from stack or re-open the PR), then re-run.");
        return Ok(Err(CmdError::UserError));
    }

    // Remove merged branches from config and delete local branches.
    let mut removed: Vec<String> = Vec::new();
    for (branch, state) in &states {
        if *state != PrState::Merged {
            continue;
        }
        remove_branch_from_config(ctx, name, branch)?;

        // If we're currently on this branch, check out base first.
        let head = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        if head == *branch {
            git_silent(ctx, &["checkout", "--quiet", base]);
        }
        if branch_exists(ctx, branch) {
            git(ctx, &["branch", "-D", branch])?;
            ok(&format!("Merged: deleted local branch '{branch}'."));
        } else {
            ok(&format!("Merged: '{branch}' already gone locally."));
        }
        removed.push(branch.to_string());
    }

    if removed.is_empty() && states.iter().all(|(_, s)| *s == PrState::Open || *s == PrState::NoPr) {
        info("Nothing to sync — no merged PRs found.");
        return Ok(Ok(()));
    }

    // Reload the stack (config has been modified) and rebase remainder.
    let updated = load_stack(ctx, name)?;
    if updated.branches.is_empty() {
        ok(&format!("All branches merged — stack '{name}' is empty."));
        return Ok(Ok(()));
    }

    step("Rebasing remaining branches...");
    do_rebase(ctx, name, remote, false) // --no-fetch: already fetched above
}
