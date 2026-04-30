use anyhow::{bail, Result};
use std::collections::HashMap;
use std::fs;

use crate::config::is_valid_stack_name;
use crate::ctx::{branch_exists, git, tip, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{err_print, info, ok, warn};

/// Detect the repo's default branch via `git rev-parse --abbrev-ref origin/HEAD`.
/// Falls back to "main" if the remote HEAD isn't configured.
pub fn detect_default_branch(ctx: &Ctx) -> String {
    git(ctx, &["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .ok()
        .and_then(|s| s.strip_prefix("origin/").map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

/// Walk ancestry from HEAD back to `base`, collecting branch names in order.
/// Returns branches ordered bottom-to-top (ready to append after the base line).
pub fn scan_branches(ctx: &Ctx, base: &str) -> Result<Vec<String>> {
    // Require a named branch (not detached HEAD).
    let head = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if head == "HEAD" {
        bail!("HEAD is detached. Check out a branch before running --scan.");
    }

    // If HEAD is already the base branch, there's nothing to scan.
    if head == base {
        return Ok(vec![]);
    }

    let base_tip = tip(ctx, base)?;

    // Build a map from commit SHA -> branch name for all local branches.
    let branch_list = git(ctx, &["branch", "--format=%(objectname) %(refname:short)"])?;
    let sha_to_branch: HashMap<String, String> = branch_list
        .lines()
        .filter_map(|line| line.trim().split_once(' '))
        .map(|(sha, name)| (sha.to_string(), name.to_string()))
        .collect();

    // Walk commits from HEAD back to (but not including) the base tip.
    // The log is newest-first; we collect branch tips as we encounter them.
    let log = git(ctx, &["log", "--format=%H", &head, &format!("^{base_tip}")])?;
    let mut stack_top_to_bottom: Vec<String> = log
        .lines()
        .filter_map(|sha| sha_to_branch.get(sha.trim()))
        .filter(|name| *name != base)
        .cloned()
        .collect();

    // Reverse so the result is bottom-to-top (matches config file order).
    stack_top_to_bottom.reverse();
    Ok(stack_top_to_bottom)
}

pub fn cmd_init(ctx: &Ctx, name: &str, base: Option<&str>, scan: bool) -> Result<CmdResult> {
    if !is_valid_stack_name(name) {
        err_print(&format!(
            "Invalid stack name '{name}'. Use letters, digits, '.', '_', '/', '-'."
        ));
        return Ok(Err(CmdError::UserError));
    }

    let detected;
    let base = match base {
        Some(b) => b,
        None => {
            detected = detect_default_branch(ctx);
            detected.as_str()
        }
    };

    if !branch_exists(ctx, base) {
        err_print(&format!("Base branch '{base}' does not exist locally."));
        return Ok(Err(CmdError::UserError));
    }
    let file = ctx.stacks_dir.join(name);
    if file.exists() {
        err_print(&format!("Stack '{name}' already exists at .stacks/{name}."));
        return Ok(Err(CmdError::UserError));
    }

    let scanned_branches = if scan {
        match scan_branches(ctx, base) {
            Ok(branches) => branches,
            Err(e) => {
                err_print(&format!("{e}"));
                return Ok(Err(CmdError::UserError));
            }
        }
    } else {
        vec![]
    };

    fs::create_dir_all(&ctx.stacks_dir)?;
    let mut content = format!(
        "# Stack: {name}\n# Base + branches in order, bottom-to-top. '#' for comments.\n{base}\n"
    );
    for b in &scanned_branches {
        content.push_str(b);
        content.push('\n');
    }
    fs::write(&file, content)?;

    ok(&format!("Created stack '{name}' with base '{base}'."));
    if scan {
        if scanned_branches.is_empty() {
            warn(&format!(
                "HEAD is already on '{base}' — no branches detected. Use 'sd add {name} <branch>' to add some."
            ));
        } else {
            info(&format!(
                "Detected {} branch{}: {}",
                scanned_branches.len(),
                if scanned_branches.len() == 1 { "" } else { "es" },
                scanned_branches.join(" -> ")
            ));
        }
    } else {
        info(&format!("Add branches with: sd add {name} <branch>"));
    }
    Ok(Ok(()))
}
