// ── Color / style escape codes ────────────────────────────────────────────────
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const BLACK: &str = "\x1b[30m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const CYAN: &str = "\x1b[36m";
pub const WHITE: &str = "\x1b[37m";
pub const BG_CYAN: &str = "\x1b[46m";

// ── Unicode tree / indicator characters ───────────────────────────────────────
pub const CIRCLE: &str = "●";
pub const CIRCLE_FILLED: &str = "◉";
pub const VERTICAL: &str = "│";
pub const BRANCH_START: &str = "╭─";
pub const BRANCH_MID: &str = "├─";
pub const BRANCH_END: &str = "└─";

// ── Standard output helpers ───────────────────────────────────────────────────
pub fn step(msg: &str) {
    eprintln!("{BOLD}{CYAN}==>{RESET} {msg}");
}

pub fn info(msg: &str) {
    eprintln!("    {msg}");
}

pub fn ok(msg: &str) {
    eprintln!("{BOLD}{GREEN}✓{RESET} {msg}");
}

pub fn warn(msg: &str) {
    eprintln!("{BOLD}{YELLOW}⚠{RESET}  {msg}");
}

pub fn err_print(msg: &str) {
    eprintln!("{BOLD}{RED}✗{RESET} {msg}");
}

// ── Tree-view branch renderer ─────────────────────────────────────────────────

/// One row in a branch tree display.
pub struct BranchRow<'a> {
    pub name: &'a str,
    /// Annotation shown after the name, e.g. "(current)" or "(missing)".
    pub tag: BranchTag,
    /// Optional second line of detail shown below the branch name.
    pub detail: Option<String>,
}

pub enum BranchTag {
    Current,
    Missing,
    Normal,
}

/// Print a header box, e.g.
///   ╭─────────────────────────────╮
///   │  <title>                    │
///   ╰─────────────────────────────╯
pub fn print_header(title: &str) {
    // Fixed 37-char inner width to match the checkout TUI
    println!("{CYAN}{BOLD}╭─────────────────────────────────────╮{RESET}");
    // Title padded to 35 chars (box inner = 37, two spaces margin)
    let padded = format!("  {title}");
    let pad_len = 37usize.saturating_sub(padded.len());
    println!("{CYAN}{BOLD}│{WHITE}{padded}{}{CYAN}│{RESET}", " ".repeat(pad_len));
    println!("{CYAN}{BOLD}╰─────────────────────────────────────╯{RESET}");
}

/// Render a list of `BranchRow`s as a colored tree with connectors.
pub fn print_branch_tree(rows: &[BranchRow<'_>]) {
    for (i, row) in rows.iter().enumerate() {
        let connector = if rows.len() == 1 {
            BRANCH_END
        } else if i == 0 {
            BRANCH_START
        } else if i == rows.len() - 1 {
            BRANCH_END
        } else {
            BRANCH_MID
        };

        let circle = match row.tag {
            BranchTag::Current => format!("{GREEN}{CIRCLE_FILLED}{RESET}"),
            BranchTag::Missing => format!("{RED}{CIRCLE}{RESET}"),
            BranchTag::Normal => format!("{BLUE}{CIRCLE}{RESET}"),
        };

        let name_styled = match row.tag {
            BranchTag::Current => format!("{BOLD}{GREEN}{}{RESET}", row.name),
            BranchTag::Missing => format!("{DIM}{RED}{}{RESET}", row.name),
            BranchTag::Normal => format!("{WHITE}{}{RESET}", row.name),
        };

        let tag_str = match row.tag {
            BranchTag::Current => format!("  {DIM}{GREEN}(current){RESET}"),
            BranchTag::Missing => format!("  {DIM}{RED}(missing){RESET}"),
            BranchTag::Normal => String::new(),
        };

        println!("{CYAN}{connector}{RESET} {circle} {name_styled}{tag_str}");

        if let Some(detail) = &row.detail {
            // Indent to align under the branch name
            println!("     {DIM}{detail}{RESET}");
        }

        // Vertical connector between items
        if i < rows.len() - 1 {
            println!("{CYAN}{VERTICAL}{RESET}");
        }
    }
}
