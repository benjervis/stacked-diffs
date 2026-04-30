use anyhow::Result;
use std::fs;

use crate::config::load_stack;
use crate::ctx::Ctx;
use crate::errors::CmdResult;
use crate::output::{info, BOLD, CYAN, DIM, RED, RESET, WHITE};

pub fn cmd_list(ctx: &Ctx) -> Result<CmdResult> {
    if !ctx.stacks_dir.exists() {
        info("No .stacks/ directory found.");
        return Ok(Ok(()));
    }
    let mut entries: Vec<_> = fs::read_dir(&ctx.stacks_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        info("No stacks configured. Add one at .stacks/<name>");
        return Ok(Ok(()));
    }

    for (idx, entry) in entries.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        let name = entry.file_name().to_string_lossy().to_string();
        match load_stack(ctx, &name) {
            Ok(stack) => {
                println!("{BOLD}{CYAN}{name}{RESET}");
                println!("  {DIM}base:{RESET} {WHITE}{}{RESET}", stack.base);
                if stack.branches.is_empty() {
                    println!("  {DIM}branches: (none yet){RESET}");
                } else {
                    // Show branches as a inline chain with arrows
                    let chain = stack.branches.join(&format!(" {DIM}→{RESET} {WHITE}"));
                    println!("  {DIM}branches:{RESET} {WHITE}{chain}{RESET}");
                }
            }
            Err(_) => println!("{BOLD}{RED}{name}{RESET} {DIM}(invalid){RESET}"),
        }
    }
    Ok(Ok(()))
}
