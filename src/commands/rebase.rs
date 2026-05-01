use anyhow::Result;
use std::fs;

use crate::config::load_stack;
use crate::ctx::{
    branch_exists, git, git_interactive, git_ok, git_silent, rebase_in_progress, repo_clean,
    short, tip, Ctx,
};
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, info, ok, step};
use crate::state::{load_tip, save_tip, state_dir};

// ---------- abort ----------

pub fn do_abort(ctx: &Ctx, name: &str) -> Result<CmdResult> {
    if rebase_in_progress(ctx)? {
        step("Aborting in-progress git rebase...");
        git_silent(ctx, &["rebase", "--abort"]);
    }
    let sd = state_dir(ctx, name)?;
    if !sd.exists() {
        info(&format!("No saved state for stack '{name}'. Nothing to restore."));
        return Ok(Ok(()));
    }
    let stack = load_stack(ctx, name)?;
    step("Restoring branches to pre-run tips...");
    for branch in &stack.branches {
        let old_tip = match load_tip(&sd, "oldtip", branch) {
            Some(t) => t,
            None => continue,
        };
        let current_tip = if branch_exists(ctx, branch) {
            Some(tip(ctx, branch)?)
        } else {
            None
        };
        if current_tip.as_deref() == Some(&old_tip) {
            info(&format!("  {branch}: already at {}", short(&old_tip)));
            continue;
        }
        git(ctx, &["update-ref", &format!("refs/heads/{branch}"), &old_tip])?;
        match &current_tip {
            Some(ct) => info(&format!("  {branch}: {} -> {}", short(ct), short(&old_tip))),
            None => info(&format!("  {branch}: -> {} (was missing)", short(&old_tip))),
        }
    }
    restore_head_and_clear_state(ctx, &sd)?;
    ok(&format!("Stack '{name}' aborted; branches restored."));
    Ok(Err(CmdError::AbortExit))
}

// ---------- rebase ----------

pub fn do_rebase(ctx: &Ctx, name: &str, remote: &str, do_fetch: bool, prefer_remote: bool) -> Result<CmdResult> {
    if !repo_clean(ctx)? {
        err_print("Working tree is dirty. Commit or stash changes before running.");
        git_interactive(ctx, &["status", "--short"])?;
        return Ok(Err(CmdError::UserError));
    }
    if rebase_in_progress(ctx)? {
        err_print("A git rebase is already in progress. Finish it ('git rebase --continue' or '--abort') before running this script.");
        return Ok(Err(CmdError::UserError));
    }

    let stack = load_stack(ctx, name)?;
    let base = &stack.base;
    let branches = &stack.branches;
    let count = branches.len();

    if count == 0 {
        err_print(&format!("Stack '{name}' has no branches. Use 'add' to create one."));
        return Ok(Err(CmdError::UserError));
    }

    let sd = state_dir(ctx, name)?;

    if sd.exists() {
        resume_existing_state(ctx, name, base, branches, &sd)?;
    } else {
        start_fresh(ctx, name, base, branches, &sd, remote, do_fetch, prefer_remote)?;
    }

    rebase_branches(ctx, name, base, branches, count, &sd)
}

/// Validate saved state still matches config when resuming after a conflict.
fn resume_existing_state(
    _ctx: &Ctx,
    name: &str,
    base: &str,
    branches: &[String],
    sd: &std::path::Path,
) -> Result<()> {
    let saved_base = fs::read_to_string(sd.join("base"))
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let saved_branches = fs::read_to_string(sd.join("branches"))
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if saved_base != base || saved_branches != branches.join("\n") {
        err_print(&format!(
            "Saved state for stack '{name}' doesn't match the current config \
             (base or branches changed). Run with --abort first to restore branches \
             and clear state, then re-run."
        ));
        return Err(anyhow::anyhow!("state mismatch"));
    }
    let completed: usize = fs::read_to_string(sd.join("next-index"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if completed < branches.len() {
        step(&format!(
            "Resuming stack '{name}' from branch '{}'.",
            branches[completed]
        ));
    } else {
        step(&format!(
            "Resuming stack '{name}' (all branches already rebased; finalising)."
        ));
    }
    Ok(())
}

/// Fetch/fast-forward the base branch, verify all branches exist, snapshot state.
fn start_fresh(
    ctx: &Ctx,
    name: &str,
    base: &str,
    branches: &[String],
    sd: &std::path::Path,
    remote: &str,
    do_fetch: bool,
    prefer_remote: bool,
) -> Result<()> {
    if do_fetch {
        fetch_and_fast_forward(ctx, base, remote)?;
        for branch in branches {
            fetch_and_fast_forward_branch(ctx, branch, remote, prefer_remote)?;
        }
    } else {
        step(&format!("Skipping fetch (--no-fetch). Using local tips."));
    }

    // Verify all branches exist locally.
    for r in std::iter::once(base).chain(branches.iter().map(|s| s.as_str())) {
        if !branch_exists(ctx, r) {
            err_print(&format!("Branch '{r}' not found locally."));
            return Err(anyhow::anyhow!("missing branch '{r}'"));
        }
    }

    snapshot_state(ctx, name, base, branches, sd)
}

/// Fetch `remote/<branch>` and fast-forward the local branch if possible.
/// If the remote has no tracking ref for this branch, silently skips.
/// If `prefer_remote` is true and local has diverged, resets local to the
/// remote tip (use when Devin Cloud has the authoritative version).
/// Otherwise divergence is an error — the user must reconcile manually.
fn fetch_and_fast_forward_branch(ctx: &Ctx, branch: &str, remote: &str, prefer_remote: bool) -> Result<()> {
    // Fetch — if the remote doesn't know this branch, that's fine.
    let fetched = git_ok(ctx, &["fetch", remote, branch])?;
    if !fetched {
        // No tracking ref on remote — nothing to pull.
        return Ok(());
    }

    let remote_ref = format!("{remote}/{branch}");
    // After fetch, check whether the remote ref actually exists.
    if !git_silent(ctx, &["rev-parse", "--verify", &remote_ref]) {
        return Ok(());
    }

    let local_tip = tip(ctx, branch)?;
    let remote_tip = tip(ctx, &remote_ref)?;

    if local_tip == remote_tip {
        return Ok(());
    }

    if git_ok(ctx, &["merge-base", "--is-ancestor", &local_tip, &remote_tip])? {
        // Local is behind remote — safe to fast-forward.
        git(ctx, &["update-ref", &format!("refs/heads/{branch}"), &remote_tip])?;
        info(&format!(
            "Fast-forwarded '{branch}' from {} to {}.",
            short(&local_tip),
            short(&remote_tip)
        ));
    } else if git_ok(ctx, &["merge-base", "--is-ancestor", &remote_tip, &local_tip])? {
        // Local is ahead of remote — nothing to do.
    } else if prefer_remote {
        // Diverged, but caller asked us to trust remote — reset local to remote tip.
        git(ctx, &["update-ref", &format!("refs/heads/{branch}"), &remote_tip])?;
        info(&format!(
            "Reset '{branch}' to {remote} tip ({}, discarding local divergence).",
            short(&remote_tip)
        ));
    } else {
        err_print(&format!(
            "Local '{branch}' has diverged from {remote}/{branch}. \
             Reconcile manually, or re-run with --prefer-remote to reset to the {remote} version."
        ));
        return Err(anyhow::anyhow!("branch diverged: {branch}"));
    }

    Ok(())
}

/// Fetch `remote/base` and fast-forward the local base branch.
pub fn fetch_and_fast_forward(ctx: &Ctx, base: &str, remote: &str) -> Result<()> {
    step(&format!("Fetching {remote}/{base}..."));
    if !git_interactive(ctx, &["fetch", remote, base])? {
        err_print("Fetch failed.");
        return Err(anyhow::anyhow!("fetch failed"));
    }
    let head = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if head == base {
        if !git_interactive(ctx, &["merge", "--ff-only", &format!("{remote}/{base}")])? {
            err_print(&format!(
                "Local '{base}' could not be fast-forwarded from {remote}/{base}. Reconcile manually."
            ));
            return Err(anyhow::anyhow!("fast-forward failed"));
        }
    } else {
        let local_tip = tip(ctx, base)?;
        let remote_ref = format!("{remote}/{base}");
        let remote_tip = tip(ctx, &remote_ref)?;
        if local_tip != remote_tip {
            if git_ok(ctx, &["merge-base", "--is-ancestor", &local_tip, &remote_tip])? {
                git(ctx, &["update-ref", &format!("refs/heads/{base}"), &remote_tip])?;
                info(&format!(
                    "Fast-forwarded '{base}' from {} to {}.",
                    short(&local_tip),
                    short(&remote_tip)
                ));
            } else {
                err_print(&format!(
                    "Local '{base}' has diverged from {remote}/{base}. Reconcile manually."
                ));
                return Err(anyhow::anyhow!("base diverged"));
            }
        }
    }
    Ok(())
}

/// Write the initial state directory (old tips, metadata).
fn snapshot_state(
    ctx: &Ctx,
    _name: &str,
    base: &str,
    branches: &[String],
    sd: &std::path::Path,
) -> Result<()> {
    fs::create_dir_all(sd)?;
    fs::write(sd.join("base"), format!("{base}\n"))?;
    fs::write(sd.join("branches"), format!("{}\n", branches.join("\n")))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    fs::write(sd.join("started-at"), format!("{now}\n"))?;

    for r in std::iter::once(base).chain(branches.iter().map(|s| s.as_str())) {
        let t = tip(ctx, r)?;
        save_tip(sd, "oldtip", r, &t)?;
    }
    let base_tip = tip(ctx, base)?;
    save_tip(sd, "newtip", base, &base_tip)?;

    let head = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let head = if head == "HEAD" {
        git(ctx, &["rev-parse", "HEAD"])?
    } else {
        head
    };
    fs::write(sd.join("original-head"), format!("{head}\n"))?;
    fs::write(sd.join("next-index"), "0\n")?;
    Ok(())
}

/// The inner rebase loop: iterate over branches, skipping completed ones.
fn rebase_branches(
    ctx: &Ctx,
    name: &str,
    base: &str,
    branches: &[String],
    count: usize,
    sd: &std::path::Path,
) -> Result<CmdResult> {
    let completed: usize = fs::read_to_string(sd.join("next-index"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    for i in completed..count {
        let branch = &branches[i];
        let parent = if i == 0 { base } else { branches[i - 1].as_str() };
        let upstream = load_tip(sd, "oldtip", parent).unwrap_or_default();
        let onto = load_tip(sd, "newtip", parent).unwrap_or_default();

        step(&format!(
            "({}/{count}) Rebasing '{branch}' onto '{parent}' (--onto {} {} {branch})",
            i + 1,
            short(&onto),
            short(&upstream)
        ));

        // Skip if branch is already correctly based on the onto tip.
        if git_ok(ctx, &["merge-base", "--is-ancestor", &onto, branch])? {
            let mb = git(ctx, &["merge-base", &onto, branch]).unwrap_or_default();
            if mb == onto {
                info(&format!("'{branch}' is already based on '{parent}' tip — skipping."));
                let branch_tip = tip(ctx, branch)?;
                save_tip(sd, "newtip", branch, &branch_tip)?;
                fs::write(sd.join("next-index"), format!("{}\n", i + 1))?;
                continue;
            }
        }

        if !git_interactive(ctx, &["rebase", "--onto", &onto, &upstream, branch])? {
            err_print(&format!("Rebase of '{branch}' hit a conflict (or failed)."));
            info("Resolve with normal git commands ('git status', 'git add', 'git rebase --continue'), then re-run:");
            info(&format!("  sd rebase {name}"));
            info("Or to bail out and restore every branch to its original tip:");
            info(&format!("  sd rebase {name} --abort"));
            return Ok(Err(CmdError::ConflictExit));
        }

        let branch_tip = tip(ctx, branch)?;
        save_tip(sd, "newtip", branch, &branch_tip)?;
        fs::write(sd.join("next-index"), format!("{}\n", i + 1))?;
    }

    restore_head_and_clear_state(ctx, sd)?;

    let plural = if count == 1 { "" } else { "es" };
    ok(&format!("Stack '{name}' rebased successfully ({count} branch{plural})."));
    let branch_list = branches.join(" ");
    info(&format!(
        "Next: review with 'git log --oneline --graph {base} {branch_list}', then 'sd push {name}'."
    ));
    Ok(Ok(()))
}

/// Check out the original HEAD (if still valid) and remove the state directory.
fn restore_head_and_clear_state(ctx: &Ctx, sd: &std::path::Path) -> Result<()> {
    let orig_head = fs::read_to_string(sd.join("original-head"))
        .ok()
        .map(|s| s.trim().to_string());
    if let Some(head) = orig_head {
        if !head.is_empty() && branch_exists(ctx, &head) {
            git_silent(ctx, &["checkout", "--quiet", &head]);
        }
    }
    fs::remove_dir_all(sd)?;
    Ok(())
}
