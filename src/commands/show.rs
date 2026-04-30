use anyhow::Result;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git, Ctx};
use crate::errors::CmdResult;
use crate::output::{print_branch_tree, print_header, BranchRow, BranchTag, BOLD, CYAN, RESET};

pub fn cmd_show(ctx: &Ctx, name: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;

    print_header(&format!("Stack: {name}"));
    println!();

    let current_branch = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();

    let all: Vec<String> = std::iter::once(stack.base.clone())
        .chain(stack.branches)
        .collect();

    if all.len() == 1 {
        // Only the base — no stacked branches yet
        let base = &all[0];
        let tag = branch_tag(ctx, base, current_branch.as_deref());
        print_branch_tree(&[BranchRow {
            name: base,
            tag,
            detail: None,
        }]);
        println!();
        println!("{CYAN}{BOLD}  base{RESET} — no stacked branches yet");
    } else {
        let rows: Vec<BranchRow<'_>> = all
            .iter()
            .map(|branch| BranchRow {
                name: branch,
                tag: branch_tag(ctx, branch, current_branch.as_deref()),
                detail: None,
            })
            .collect();
        print_branch_tree(&rows);
    }

    Ok(Ok(()))
}

fn branch_tag(ctx: &Ctx, branch: &str, current: Option<&str>) -> BranchTag {
    if current == Some(branch) {
        BranchTag::Current
    } else if !branch_exists(ctx, branch) {
        BranchTag::Missing
    } else {
        BranchTag::Normal
    }
}
