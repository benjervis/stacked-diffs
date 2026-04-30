use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

// ---------- CLI definition ----------
// We use manual arg parsing for the top-level dispatch so that:
//   - `--list` and `--help` work without a subcommand
//   - Bare `<stack> [flags]` is treated as `rebase <stack> [flags]`
//   - Subcommands are dispatched via clap for proper help/error messages
//
// The Cli struct is only used when a recognised subcommand is present.

#[derive(Parser)]
#[command(
    name = "sd",
    about = "Manage stacked git branches",
    long_about = None,
    disable_version_flag = true,
    disable_help_flag = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new stack config (default base: main)
    Init {
        stack: String,
        base: Option<String>,
    },
    /// Create a branch off the top of the stack and append to config
    Add { stack: String, branch: String },
    /// Remove a branch from the config (does NOT delete the local branch)
    Rm { stack: String, branch: String },
    /// Print the stack chain in one line
    Show { stack: String },
    /// Show per-branch tip + ahead/behind state
    Status {
        stack: String,
        #[arg(long)]
        remote: Option<String>,
    },
    /// Rebase every branch in the stack onto its parent
    Rebase {
        stack: String,
        #[arg(long = "no-fetch")]
        no_fetch: bool,
        #[arg(long)]
        remote: Option<String>,
        #[arg(long)]
        abort: bool,
    },
    /// git push --force-with-lease each branch
    Push {
        stack: String,
        #[arg(long)]
        remote: Option<String>,
    },
}

// ---------- context ----------

struct Ctx {
    repo_root: PathBuf,
    stacks_dir: PathBuf,
}

impl Ctx {
    fn new() -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("failed to run git")?;
        if !output.status.success() {
            bail!("Not inside a git repository.");
        }
        let repo_root = PathBuf::from(String::from_utf8(output.stdout)?.trim());
        let stacks_dir = repo_root.join(".stacks");
        Ok(Ctx {
            repo_root,
            stacks_dir,
        })
    }
}

// ---------- output helpers ----------

fn step(msg: &str) {
    eprintln!("\x1b[1;36m==>\x1b[0m {msg}");
}

fn info(msg: &str) {
    eprintln!("    {msg}");
}

fn ok(msg: &str) {
    eprintln!("\x1b[1;32m✓\x1b[0m {msg}");
}

fn err_print(msg: &str) {
    eprintln!("\x1b[1;31m✗\x1b[0m {msg}");
}

// ---------- git helpers ----------

fn git(ctx: &Ctx, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&ctx.repo_root).args(args);
    let out = cmd.output().context("git invocation failed")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

fn git_q(ctx: &Ctx, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(&ctx.repo_root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn git_ok(ctx: &Ctx, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(&ctx.repo_root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("git invocation failed")?
        .success())
}

/// Run git, streaming stdout/stderr to the terminal. Returns success bool.
fn git_interactive(ctx: &Ctx, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(&ctx.repo_root)
        .args(args)
        .status()
        .context("git invocation failed")?
        .success())
}

fn branch_exists(ctx: &Ctx, branch: &str) -> bool {
    git_q(ctx, &["show-ref", "--verify", &format!("refs/heads/{branch}")])
}

fn tip(ctx: &Ctx, r#ref: &str) -> Result<String> {
    git(ctx, &["rev-parse", "--verify", &format!("{ref}^{{commit}}")])
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

fn repo_clean(ctx: &Ctx) -> Result<bool> {
    let out = git(ctx, &["status", "--porcelain", "--untracked-files=no"])?;
    Ok(out.is_empty())
}

fn rebase_in_progress(ctx: &Ctx) -> Result<bool> {
    let gd = git(ctx, &["rev-parse", "--git-dir"])?;
    // git-dir may be relative; resolve against repo_root
    let gd_path = if Path::new(&gd).is_absolute() {
        PathBuf::from(&gd)
    } else {
        ctx.repo_root.join(&gd)
    };
    Ok(gd_path.join("rebase-merge").exists() || gd_path.join("rebase-apply").exists())
}

// ---------- config ----------

struct Stack {
    base: String,
    branches: Vec<String>,
}

fn load_stack(ctx: &Ctx, name: &str) -> Result<Stack> {
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
    {
        bail!("Invalid stack name '{name}'.");
    }
    let file = ctx.stacks_dir.join(name);
    if !file.exists() {
        bail!("No stack config at .stacks/{name}. Create one or run with --list.");
    }
    let reader = io::BufReader::new(fs::File::open(&file)?);
    let mut base = String::new();
    let mut branches: Vec<String> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if base.is_empty() {
            base = line;
        } else {
            branches.push(line);
        }
    }
    if base.is_empty() {
        bail!("Stack '{name}' has no base.");
    }
    if !branches.is_empty() && branches.contains(&base) {
        bail!("Stack '{name}': base '{base}' must not also appear in branches.");
    }
    if branches.len() > 1 {
        let mut sorted = branches.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != branches.len() {
            bail!("Stack '{name}' has duplicate branches.");
        }
    }
    Ok(Stack { base, branches })
}

// ---------- state ----------

fn enc(r#ref: &str) -> String {
    r#ref.replace('/', "__SLASH__")
}

fn state_dir(ctx: &Ctx, name: &str) -> Result<PathBuf> {
    let gd = git(ctx, &["rev-parse", "--git-dir"])?;
    let gd_path = if Path::new(&gd).is_absolute() {
        PathBuf::from(&gd)
    } else {
        ctx.repo_root.join(&gd)
    };
    Ok(gd_path.join(format!("stack-rebase-{}", enc(name))))
}

fn save_tip(state_dir: &Path, kind: &str, branch: &str, sha: &str) -> Result<()> {
    let dir = state_dir.join(kind);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(enc(branch)), format!("{sha}\n"))?;
    Ok(())
}

fn load_tip(state_dir: &Path, kind: &str, branch: &str) -> Option<String> {
    let f = state_dir.join(kind).join(enc(branch));
    fs::read_to_string(f).ok().map(|s| s.trim().to_string())
}

// ---------- subcommands ----------

fn cmd_help() {
    println!(
        r#"Usage:
  sd <command> [args]

Commands:
  init <stack> [<base>]              create a new stack config (default base: main)
  add <stack> <branch>               create a branch off the top of the stack
                                     and append it to the config
  rm <stack> <branch>                remove a branch from the stack config
                                     (does NOT delete the local git branch)
  show <stack>                       print the stack chain in one line
  status <stack> [--remote NAME]     per-branch tip + ahead/behind state
  rebase <stack> [flags]             rebase the whole stack
      --no-fetch                       don't fetch & fast-forward <base>
      --remote NAME                    remote to fetch from (default: origin)
      --abort                          restore branches to pre-run tips, clear state
  push <stack> [--remote NAME]       git push --force-with-lease each branch
  --list                             list configured stacks
  --help                             show this help

Backwards compat: bare '<stack>' is shorthand for 'rebase <stack>'.

Stack config: .stacks/<stack-name>
  First non-comment line: base branch (e.g. main)
  Remaining non-comment lines: stack branches in order, bottom to top
  Lines starting with '#' and blank lines are ignored"#
    );
}

fn cmd_list(ctx: &Ctx) -> Result<i32> {
    if !ctx.stacks_dir.exists() {
        info("No .stacks/ directory found.");
        return Ok(0);
    }
    let mut found = false;
    let mut entries: Vec<_> = fs::read_dir(&ctx.stacks_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        found = true;
        let name = entry.file_name().to_string_lossy().to_string();
        match load_stack(ctx, &name) {
            Ok(stack) => {
                println!("{name}");
                println!("  base: {}", stack.base);
                if stack.branches.is_empty() {
                    println!("  branches: (none yet)");
                } else {
                    println!("  branches: {}", stack.branches.join(" -> "));
                }
            }
            Err(_) => {
                println!("{name} (invalid)");
            }
        }
    }
    if !found {
        info("No stacks configured. Add one at .stacks/<name>");
    }
    Ok(0)
}

fn do_abort(ctx: &Ctx, name: &str) -> Result<i32> {
    if rebase_in_progress(ctx)? {
        step("Aborting in-progress git rebase...");
        git_q(ctx, &["rebase", "--abort"]);
    }
    let sd = state_dir(ctx, name)?;
    if !sd.exists() {
        info(&format!("No saved state for stack '{name}'. Nothing to restore."));
        return Ok(0);
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
            info(&format!(
                "  {branch}: already at {}",
                short(&old_tip)
            ));
            continue;
        }
        git(ctx, &["update-ref", &format!("refs/heads/{branch}"), &old_tip])?;
        match &current_tip {
            Some(ct) => info(&format!(
                "  {branch}: {} -> {}",
                short(ct),
                short(&old_tip)
            )),
            None => info(&format!("  {branch}: -> {} (was missing)", short(&old_tip))),
        }
    }
    let orig_head = fs::read_to_string(sd.join("original-head"))
        .ok()
        .map(|s| s.trim().to_string());
    if let Some(head) = orig_head {
        if !head.is_empty() && branch_exists(ctx, &head) {
            git_q(ctx, &["checkout", "--quiet", &head]);
        }
    }
    fs::remove_dir_all(&sd)?;
    ok(&format!("Stack '{name}' aborted; branches restored."));
    Ok(3)
}

fn do_rebase(ctx: &Ctx, name: &str, remote: &str, do_fetch: bool) -> Result<i32> {
    if !repo_clean(ctx)? {
        err_print("Working tree is dirty. Commit or stash changes before running.");
        git_interactive(ctx, &["status", "--short"])?;
        return Ok(1);
    }
    if rebase_in_progress(ctx)? {
        err_print("A git rebase is already in progress. Finish it ('git rebase --continue' or '--abort') before running this script.");
        return Ok(1);
    }

    let stack = load_stack(ctx, name)?;
    let base = &stack.base;
    let branches = &stack.branches;
    let count = branches.len();

    if count == 0 {
        err_print(&format!("Stack '{name}' has no branches. Use 'add' to create one."));
        return Ok(1);
    }

    let sd = state_dir(ctx, name)?;

    if sd.exists() {
        // Validate state matches config
        let saved_base = fs::read_to_string(sd.join("base"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let saved_branches = fs::read_to_string(sd.join("branches"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let current_branches = branches.join("\n");
        if saved_base != *base || saved_branches != current_branches {
            err_print(&format!("Saved state for stack '{name}' doesn't match the current config (base or branches changed). Run with --abort first to restore branches and clear state, then re-run."));
            return Ok(1);
        }
        let completed: usize = fs::read_to_string(sd.join("next-index"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if completed < count {
            step(&format!(
                "Resuming stack '{name}' from branch '{}'.",
                branches[completed]
            ));
        } else {
            step(&format!(
                "Resuming stack '{name}' (all branches already rebased; finalising)."
            ));
        }
    } else {
        if do_fetch {
            step(&format!("Fetching {remote}/{base}..."));
            if !git_interactive(ctx, &["fetch", remote, base])? {
                err_print("Fetch failed.");
                return Ok(1);
            }
            let head = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])?;
            if head == *base {
                if !git_interactive(ctx, &["merge", "--ff-only", &format!("{remote}/{base}")])? {
                    err_print(&format!(
                        "Local '{base}' could not be fast-forwarded from {remote}/{base}. Reconcile manually."
                    ));
                    return Ok(1);
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
                        return Ok(1);
                    }
                }
            }
        } else {
            step(&format!("Skipping fetch (--no-fetch). Using local '{base}' tip."));
        }

        // Verify all branches exist
        for r in std::iter::once(base.as_str()).chain(branches.iter().map(|s| s.as_str())) {
            if !branch_exists(ctx, r) {
                err_print(&format!("Branch '{r}' not found locally."));
                return Ok(1);
            }
        }

        // Snapshot tips & write state
        fs::create_dir_all(&sd)?;
        fs::write(sd.join("base"), format!("{base}\n"))?;
        fs::write(sd.join("branches"), format!("{}\n", branches.join("\n")))?;
        // started-at
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        fs::write(sd.join("started-at"), format!("{now}\n"))?;

        for r in std::iter::once(base.as_str()).chain(branches.iter().map(|s| s.as_str())) {
            let t = tip(ctx, r)?;
            save_tip(&sd, "oldtip", r, &t)?;
        }
        let base_tip = tip(ctx, base)?;
        save_tip(&sd, "newtip", base, &base_tip)?;

        let head = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let head = if head == "HEAD" {
            git(ctx, &["rev-parse", "HEAD"])?
        } else {
            head
        };
        fs::write(sd.join("original-head"), format!("{head}\n"))?;
        fs::write(sd.join("next-index"), "0\n")?;
    }

    let completed: usize = fs::read_to_string(sd.join("next-index"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    for i in 0..count {
        if i < completed {
            continue;
        }
        let branch = &branches[i];
        let parent = if i == 0 {
            base.as_str()
        } else {
            branches[i - 1].as_str()
        };
        let upstream = load_tip(&sd, "oldtip", parent).unwrap_or_default();
        let onto = load_tip(&sd, "newtip", parent).unwrap_or_default();

        step(&format!(
            "({}/{count}) Rebasing '{branch}' onto '{parent}' (--onto {} {} {branch})",
            i + 1,
            short(&onto),
            short(&upstream)
        ));

        // Skip if branch is already correctly based on onto
        if git_ok(ctx, &["merge-base", "--is-ancestor", &onto, branch])? {
            let mb = git(ctx, &["merge-base", &onto, branch]).unwrap_or_default();
            if mb == onto {
                info(&format!(
                    "'{branch}' is already based on '{parent}' tip — skipping."
                ));
                let branch_tip = tip(ctx, branch)?;
                save_tip(&sd, "newtip", branch, &branch_tip)?;
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
            return Ok(2);
        }

        let branch_tip = tip(ctx, branch)?;
        save_tip(&sd, "newtip", branch, &branch_tip)?;
        fs::write(sd.join("next-index"), format!("{}\n", i + 1))?;
    }

    let orig_head = fs::read_to_string(sd.join("original-head"))
        .ok()
        .map(|s| s.trim().to_string());
    if let Some(head) = orig_head {
        if !head.is_empty() && branch_exists(ctx, &head) {
            git_q(ctx, &["checkout", "--quiet", &head]);
        }
    }
    fs::remove_dir_all(&sd)?;

    let plural = if count == 1 { "" } else { "es" };
    ok(&format!("Stack '{name}' rebased successfully ({count} branch{plural})."));
    let branch_list = branches.join(" ");
    info(&format!(
        "Next: review with 'git log --oneline --graph {base} {branch_list}', then 'git push --force-with-lease' each branch."
    ));
    Ok(0)
}

fn cmd_init(ctx: &Ctx, name: &str, base: Option<&str>) -> Result<i32> {
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
    {
        err_print(&format!(
            "Invalid stack name '{name}'. Use letters, digits, '.', '_', '/', '-'."
        ));
        return Ok(1);
    }
    let base = base.unwrap_or("main");
    if !branch_exists(ctx, base) {
        err_print(&format!("Base branch '{base}' does not exist locally."));
        return Ok(1);
    }
    let file = ctx.stacks_dir.join(name);
    if file.exists() {
        err_print(&format!("Stack '{name}' already exists at .stacks/{name}."));
        return Ok(1);
    }
    fs::create_dir_all(&ctx.stacks_dir)?;
    let content = format!("# Stack: {name}\n# Base + branches in order, bottom-to-top. '#' for comments.\n{base}\n");
    fs::write(&file, content)?;
    ok(&format!("Created stack '{name}' with base '{base}'."));
    info(&format!("Add branches with: sd add {name} <branch>"));
    Ok(0)
}

fn cmd_add(ctx: &Ctx, name: &str, branch: &str) -> Result<i32> {
    let stack = load_stack(ctx, name)?;
    if branch == stack.base {
        err_print(&format!(
            "Branch '{branch}' is the base of stack '{name}'."
        ));
        return Ok(1);
    }
    if stack.branches.contains(&branch.to_string()) {
        err_print(&format!("Branch '{branch}' is already in stack '{name}'."));
        return Ok(1);
    }
    if branch_exists(ctx, branch) {
        err_print(&format!(
            "Branch '{branch}' already exists locally. Choose a different name or delete it first."
        ));
        return Ok(1);
    }
    if !repo_clean(ctx)? {
        err_print("Working tree is dirty. Commit or stash changes first.");
        git_interactive(ctx, &["status", "--short", "--untracked-files=no"])?;
        return Ok(1);
    }
    let top = if stack.branches.is_empty() {
        stack.base.clone()
    } else {
        stack.branches.last().unwrap().clone()
    };
    if !git_ok(ctx, &["checkout", &top])? {
        err_print(&format!("Couldn't checkout '{top}'."));
        return Ok(1);
    }
    if !git_ok(ctx, &["checkout", "-b", branch])? {
        err_print(&format!("Couldn't create branch '{branch}' from '{top}'."));
        return Ok(1);
    }
    let file = ctx.stacks_dir.join(name);
    let mut content = fs::read_to_string(&file)?;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(branch);
    content.push('\n');
    fs::write(&file, content)?;
    ok(&format!(
        "Created '{branch}' from '{top}' and added to stack '{name}'."
    ));
    Ok(0)
}

fn cmd_rm(ctx: &Ctx, name: &str, branch: &str) -> Result<i32> {
    let stack = load_stack(ctx, name)?;
    if !stack.branches.contains(&branch.to_string()) {
        err_print(&format!("Branch '{branch}' is not in stack '{name}'."));
        return Ok(1);
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
    let file = ctx.stacks_dir.join(name);
    let content = fs::read_to_string(&file)?;
    let new_content: String = content
        .lines()
        .filter(|line| line.trim() != branch)
        .flat_map(|line| [line, "\n"])
        .collect();
    fs::write(&file, new_content)?;
    ok(&format!("Removed '{branch}' from stack '{name}'."));
    info(&format!(
        "The local git branch '{branch}' was not deleted. Use 'git branch -D {branch}' if you also want to delete it."
    ));
    Ok(0)
}

fn cmd_show(ctx: &Ctx, name: &str) -> Result<i32> {
    let stack = load_stack(ctx, name)?;
    println!("Stack: {name}");
    println!("  base: {}", stack.base);
    if stack.branches.is_empty() {
        println!("  branches: (none yet)");
    } else {
        println!("  branches: {}", stack.branches.join(" -> "));
    }
    Ok(0)
}

fn cmd_status(ctx: &Ctx, name: &str, remote: &str) -> Result<i32> {
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
            let remote_info = if git_q(
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
    Ok(0)
}

fn cmd_push(ctx: &Ctx, name: &str, remote: &str) -> Result<i32> {
    let stack = load_stack(ctx, name)?;
    let count = stack.branches.len();
    if count == 0 {
        err_print(&format!("Stack '{name}' has no branches to push."));
        return Ok(1);
    }
    for (i, branch) in stack.branches.iter().enumerate() {
        if !branch_exists(ctx, branch) {
            err_print(&format!("Branch '{branch}' missing locally; aborting push."));
            return Ok(1);
        }
        step(&format!(
            "({}/{count}) Pushing '{branch}' to '{remote}' (--force-with-lease)...",
            i + 1
        ));
        if !git_interactive(ctx, &["push", "--force-with-lease", remote, branch])? {
            err_print(&format!("Push of '{branch}' failed."));
            return Ok(1);
        }
    }
    let plural = if count == 1 { "" } else { "es" };
    ok(&format!("Pushed {count} branch{plural} to '{remote}'."));
    Ok(0)
}

// ---------- main ----------

/// Known subcommand names — used to distinguish `sd rebase foo` from `sd foo` (legacy).
const SUBCOMMANDS: &[&str] = &["init", "add", "rm", "show", "status", "rebase", "push"];

fn run() -> Result<i32> {
    let raw: Vec<String> = std::env::args().collect();
    // args[0] is the binary name; work with args[1..]
    let args = &raw[1..];

    // No arguments → help
    if args.is_empty() {
        cmd_help();
        return Ok(0);
    }

    let first = &args[0];

    // --help / -h
    if first == "--help" || first == "-h" {
        cmd_help();
        return Ok(0);
    }

    // --list
    if first == "--list" {
        let ctx = Ctx::new()?;
        return cmd_list(&ctx);
    }

    // Unknown top-level flags (e.g. --bogus)
    if first.starts_with('-') && !SUBCOMMANDS.contains(&first.as_str()) {
        err_print(&format!("Unknown flag: {first}"));
        return Ok(1);
    }

    // If the first arg is a known subcommand, hand off to clap for the subcommand.
    if SUBCOMMANDS.contains(&first.as_str()) {
        let cli = Cli::try_parse().map_err(|e| anyhow::anyhow!("{e}"))?;
        let ctx = Ctx::new()?;
        return match cli.command {
            Cmd::Init { stack, base } => cmd_init(&ctx, &stack, base.as_deref()),
            Cmd::Add { stack, branch } => cmd_add(&ctx, &stack, &branch),
            Cmd::Rm { stack, branch } => cmd_rm(&ctx, &stack, &branch),
            Cmd::Show { stack } => cmd_show(&ctx, &stack),
            Cmd::Status { stack, remote } => {
                cmd_status(&ctx, &stack, remote.as_deref().unwrap_or("origin"))
            }
            Cmd::Rebase {
                stack,
                no_fetch,
                remote,
                abort,
            } => {
                let remote = remote.as_deref().unwrap_or("origin");
                if abort {
                    do_abort(&ctx, &stack)
                } else {
                    do_rebase(&ctx, &stack, remote, !no_fetch)
                }
            }
            Cmd::Push { stack, remote } => {
                cmd_push(&ctx, &stack, remote.as_deref().unwrap_or("origin"))
            }
        };
    }

    // Legacy: bare <stack> [flags] is shorthand for `rebase <stack> [flags]`
    let stack = first.clone();
    let rest = &args[1..];
    let mut no_fetch = false;
    let mut remote = "origin".to_string();
    let mut abort = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--no-fetch" => no_fetch = true,
            "--fetch" => no_fetch = false,
            "--abort" => abort = true,
            "--remote" => {
                i += 1;
                if i >= rest.len() {
                    err_print("--remote requires an argument");
                    return Ok(1);
                }
                remote = rest[i].clone();
            }
            f if f.starts_with('-') => {
                err_print(&format!("Unknown flag: {f}"));
                return Ok(1);
            }
            other => {
                err_print(&format!(
                    "Unexpected positional argument '{other}' (already got '{stack}')."
                ));
                return Ok(1);
            }
        }
        i += 1;
    }
    let ctx = Ctx::new()?;
    if abort {
        do_abort(&ctx, &stack)
    } else {
        do_rebase(&ctx, &stack, &remote, !no_fetch)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            err_print(&format!("{e:#}"));
            ExitCode::from(1)
        }
    }
}
