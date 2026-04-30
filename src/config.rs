use anyhow::{bail, Result};
use std::fs;
use std::io::{self, BufRead};

use crate::ctx::Ctx;

pub struct Stack {
    pub base: String,
    pub branches: Vec<String>,
}

pub fn load_stack(ctx: &Ctx, name: &str) -> Result<Stack> {
    if !is_valid_stack_name(name) {
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
        let line = line?.trim().to_string();
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

/// Remove every line matching `branch` from the stack's config file.
///
/// This is the single source of truth for branch removal — used by both
/// `cmd_rm` and `cmd_sync`.
pub fn remove_branch_from_config(ctx: &Ctx, name: &str, branch: &str) -> Result<()> {
    let file = ctx.stacks_dir.join(name);
    let content = fs::read_to_string(&file)?;
    let new_content: String = content
        .lines()
        .filter(|line| line.trim() != branch)
        .flat_map(|line| [line, "\n"])
        .collect();
    fs::write(&file, new_content)?;
    Ok(())
}

pub fn is_valid_stack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

/// Resolve a stack name: use the provided one, or auto-detect if there's
/// exactly one stack in .stacks/. Errors clearly if zero or >1 stacks exist.
pub fn resolve_stack(ctx: &Ctx, name: Option<&str>) -> Result<String> {
    if let Some(n) = name {
        return Ok(n.to_string());
    }
    if !ctx.stacks_dir.exists() {
        bail!("No .stacks/ directory found. Create a stack with 'sd init <stack>'.");
    }
    let mut stacks: Vec<String> = fs::read_dir(&ctx.stacks_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    stacks.sort();
    match stacks.len() {
        0 => bail!("No stacks configured. Create one with 'sd init <stack>'."),
        1 => Ok(stacks.remove(0)),
        _ => bail!(
            "Multiple stacks found ({}). Specify one explicitly.",
            stacks.join(", ")
        ),
    }
}
