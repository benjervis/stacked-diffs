use anyhow::Result;
use std::fs;

use crate::config::load_stack;
use crate::ctx::Ctx;
use crate::errors::CmdResult;
use crate::output::info;

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

    for entry in entries {
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
            Err(_) => println!("{name} (invalid)"),
        }
    }
    Ok(Ok(()))
}
