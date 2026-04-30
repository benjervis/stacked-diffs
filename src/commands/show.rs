use anyhow::Result;

use crate::config::load_stack;
use crate::ctx::Ctx;
use crate::errors::CmdResult;

pub fn cmd_show(ctx: &Ctx, name: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    println!("Stack: {name}");
    println!("  base: {}", stack.base);
    if stack.branches.is_empty() {
        println!("  branches: (none yet)");
    } else {
        println!("  branches: {}", stack.branches.join(" -> "));
    }
    Ok(Ok(()))
}
