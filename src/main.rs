mod cli;
mod commands;
mod config;
mod ctx;
mod errors;
mod output;
mod state;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::process::ExitCode;

use cli::{Cli, Cmd, SUBCOMMANDS};
use commands::{
    add::cmd_add, checkout::cmd_checkout, completions::cmd_completions, init::cmd_init,
    list::cmd_list, push::cmd_push, rebase::{do_abort, do_rebase}, rm::cmd_rm, show::cmd_show,
    status::cmd_status, sync::cmd_sync,
};
use config::resolve_stack;
use ctx::Ctx;
use errors::{CmdError, CmdResult};
use output::err_print;

// ---------- help ----------

fn cmd_help() {
    // Print clap's generated help, which always stays in sync with the CLI definition.
    let mut cmd = Cli::command();
    let _ = cmd.print_long_help();
    println!();
    println!("Backwards compat: bare '<stack>' is shorthand for 'rebase <stack>'.");
}

// ---------- top-level dispatch ----------

fn run() -> Result<CmdResult> {
    let raw: Vec<String> = std::env::args().collect();
    let args = &raw[1..];

    if args.is_empty() {
        cmd_help();
        return Ok(Ok(()));
    }

    let first = &args[0];

    if first == "--help" || first == "-h" {
        cmd_help();
        return Ok(Ok(()));
    }

    if first == "--list" {
        let ctx = Ctx::new()?;
        return cmd_list(&ctx);
    }

    if first.starts_with('-') && !SUBCOMMANDS.contains(&first.as_str()) {
        err_print(&format!("Unknown flag: {first}"));
        return Ok(Err(CmdError::UserError));
    }

    if SUBCOMMANDS.contains(&first.as_str()) {
        let cli = Cli::try_parse().map_err(|e| anyhow::anyhow!("{e}"))?;
        let ctx = Ctx::new()?;
        return dispatch(cli.command, &ctx);
    }

    // Legacy: bare `<stack> [flags]` is shorthand for `rebase <stack> [flags]`
    rebase_legacy(args)
}

fn dispatch(cmd: Cmd, ctx: &Ctx) -> Result<CmdResult> {
    match cmd {
        Cmd::Init { stack, base, scan } => {
            let Some(stack) = stack else {
                err_print("init: stack name required.");
                return Ok(Err(CmdError::UserError));
            };
            cmd_init(ctx, &stack, base.as_deref(), scan)
        }
        Cmd::Add { stack_or_branch, branch } => {
            let (stack, branch) = match branch {
                Some(b) => (stack_or_branch, b),
                None => (resolve_stack(ctx, None)?, stack_or_branch),
            };
            cmd_add(ctx, &stack, &branch)
        }
        Cmd::Rm { stack_or_branch, branch } => {
            let (stack, branch) = match branch {
                Some(b) => (stack_or_branch, b),
                None => (resolve_stack(ctx, None)?, stack_or_branch),
            };
            cmd_rm(ctx, &stack, &branch)
        }
        Cmd::Show { stack } => {
            let stack = resolve_stack(ctx, stack.as_deref())?;
            cmd_show(ctx, &stack)
        }
        Cmd::Status { stack, remote } => {
            let stack = resolve_stack(ctx, stack.as_deref())?;
            cmd_status(ctx, &stack, remote.as_deref().unwrap_or("origin"))
        }
        Cmd::Rebase { stack, no_fetch, remote, abort } => {
            let stack = resolve_stack(ctx, stack.as_deref())?;
            let remote = remote.as_deref().unwrap_or("origin");
            if abort {
                do_abort(ctx, &stack)
            } else {
                do_rebase(ctx, &stack, remote, !no_fetch)
            }
        }
        Cmd::Push { stack, remote } => {
            let stack = resolve_stack(ctx, stack.as_deref())?;
            cmd_push(ctx, &stack, remote.as_deref().unwrap_or("origin"))
        }
        Cmd::Sync { stack, remote } => {
            let stack = resolve_stack(ctx, stack.as_deref())?;
            cmd_sync(ctx, &stack, remote.as_deref().unwrap_or("origin"))
        }
        Cmd::Checkout { stack } => {
            let stack = resolve_stack(ctx, stack.as_deref())?;
            cmd_checkout(ctx, &stack)
        }
        Cmd::Completions { shell } => {
            cmd_completions(shell);
            Ok(Ok(()))
        }
    }
}

/// Handle the legacy `sd <stack> [rebase flags]` shorthand.
fn rebase_legacy(args: &[String]) -> Result<CmdResult> {
    let stack = args[0].clone();
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
                    return Ok(Err(CmdError::UserError));
                }
                remote = rest[i].clone();
            }
            f if f.starts_with('-') => {
                err_print(&format!("Unknown flag: {f}"));
                return Ok(Err(CmdError::UserError));
            }
            other => {
                err_print(&format!(
                    "Unexpected positional argument '{other}' (already got '{stack}')."
                ));
                return Ok(Err(CmdError::UserError));
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

// ---------- entry point ----------

fn main() -> ExitCode {
    match run() {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e)) => ExitCode::from(e.exit_code()),
        Err(e) => {
            err_print(&format!("{e:#}"));
            ExitCode::from(1)
        }
    }
}
