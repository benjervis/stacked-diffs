use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Ctx {
    pub repo_root: PathBuf,
    pub stacks_dir: PathBuf,
}

impl Ctx {
    pub fn new() -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("failed to run git")?;
        if !output.status.success() {
            bail!("Not inside a git repository.");
        }
        let repo_root = PathBuf::from(String::from_utf8(output.stdout)?.trim());
        let stacks_dir = repo_root.join(".stacks");
        Ok(Ctx { repo_root, stacks_dir })
    }
}

// ---------- git helpers ----------

/// Run git, capturing stdout. Returns trimmed output or an error.
pub fn git(ctx: &Ctx, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C").arg(&ctx.repo_root)
        .args(args)
        .output()
        .context("git invocation failed")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

/// Run git, discarding all output. Returns `false` on failure without propagating errors.
pub fn git_silent(ctx: &Ctx, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C").arg(&ctx.repo_root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run git, discarding all output. Propagates spawn errors; returns the success bool.
pub fn git_ok(ctx: &Ctx, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C").arg(&ctx.repo_root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("git invocation failed")?
        .success())
}

/// Run git, streaming stdout/stderr to the terminal. Returns the success bool.
pub fn git_interactive(ctx: &Ctx, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C").arg(&ctx.repo_root)
        .args(args)
        .status()
        .context("git invocation failed")?
        .success())
}

// ---------- derived helpers ----------

pub fn branch_exists(ctx: &Ctx, branch: &str) -> bool {
    git_silent(ctx, &["show-ref", "--verify", &format!("refs/heads/{branch}")])
}

pub fn tip(ctx: &Ctx, r#ref: &str) -> Result<String> {
    git(ctx, &["rev-parse", "--verify", &format!("{ref}^{{commit}}")])
}

/// Shorten a SHA to 8 characters.
pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

pub fn repo_clean(ctx: &Ctx) -> Result<bool> {
    let out = git(ctx, &["status", "--porcelain", "--untracked-files=no"])?;
    Ok(out.is_empty())
}

pub fn rebase_in_progress(ctx: &Ctx) -> Result<bool> {
    let gd = git(ctx, &["rev-parse", "--git-dir"])?;
    let gd_path = if Path::new(&gd).is_absolute() {
        PathBuf::from(&gd)
    } else {
        ctx.repo_root.join(&gd)
    };
    Ok(gd_path.join("rebase-merge").exists() || gd_path.join("rebase-apply").exists())
}
