use clap::{Parser, Subcommand};
use clap_complete::Shell;

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
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Create a new stack config (default base: auto-detected from origin/HEAD)
    Init {
        stack: Option<String>,
        /// Override the base branch
        #[arg(long)]
        base: Option<String>,
        /// Walk ancestry from HEAD to base and populate the config
        #[arg(long)]
        scan: bool,
    },
    /// Create a branch off the top of the stack and append to config
    ///
    /// With one argument: sd add <branch>  (auto-detects stack if only one exists)
    /// With two arguments: sd add <stack> <branch>
    Add {
        /// Stack name, or branch name if only one stack exists
        stack_or_branch: String,
        /// Branch name (required when stack name is also given)
        branch: Option<String>,
    },
    /// Remove a branch from the config (does NOT delete the local branch)
    ///
    /// With one argument: sd rm <branch>  (auto-detects stack if only one exists)
    /// With two arguments: sd rm <stack> <branch>
    Rm {
        /// Stack name, or branch name if only one stack exists
        stack_or_branch: String,
        /// Branch name (required when stack name is also given)
        branch: Option<String>,
    },
    /// Print the stack chain in one line
    Show { stack: Option<String> },
    /// Show per-branch tip + ahead/behind state
    Status {
        stack: Option<String>,
        #[arg(long)]
        remote: Option<String>,
    },
    /// Rebase every branch in the stack onto its parent
    Rebase {
        stack: Option<String>,
        #[arg(long = "no-fetch")]
        no_fetch: bool,
        #[arg(long)]
        remote: Option<String>,
        #[arg(long)]
        abort: bool,
    },
    /// git push --force-with-lease each branch
    Push {
        stack: Option<String>,
        #[arg(long)]
        remote: Option<String>,
    },
    /// Sync the stack: detect merged PRs, clean up, and rebase remaining branches
    Sync {
        stack: Option<String>,
        #[arg(long)]
        remote: Option<String>,
    },
    /// Print shell completion script to stdout
    #[command(hide = true)]
    Completions { shell: Shell },
}

/// Known subcommand names — used to distinguish `sd rebase foo` from `sd foo` (legacy shorthand).
pub const SUBCOMMANDS: &[&str] = &[
    "init", "add", "rm", "show", "status", "rebase", "push", "sync", "completions",
];
