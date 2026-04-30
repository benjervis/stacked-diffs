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
    /// Extra lines printed below the branch name, already styled.
    /// Each entry is printed as its own indented line; no extra styling is added.
    pub detail: Vec<String>,
}

pub enum BranchTag {
    Current,
    Missing,
    Normal,
}

/// Strip ANSI escape sequences from `s` to get the visible character count.
fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
            // skip all chars inside an escape sequence
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            len += 1;
        }
    }
    len
}

/// Print a header box:
///   ╭─────────────────────────────────────╮
///   │  <title>  <subtitle>                │
///   ╰─────────────────────────────────────╯
///
/// `title` is shown in white/bold; `subtitle` (optional) is shown dim.
/// Both may contain ANSI codes — visible width is measured correctly.
pub fn print_header(title: &str, subtitle: Option<&str>) {
    const INNER: usize = 37; // visible chars between the two │ borders

    println!("{CYAN}{BOLD}╭─────────────────────────────────────╮{RESET}");

    let title_part = format!("{WHITE}{BOLD}{title}{RESET}");
    let sub_part = match subtitle {
        Some(s) => format!("  {DIM}{s}{RESET}"),
        None => String::new(),
    };
    // Two leading spaces of margin
    let content = format!("  {title_part}{sub_part}");
    let visible = 2 + visible_len(title) + if subtitle.is_some() { 2 + visible_len(subtitle.unwrap()) } else { 0 };
    let pad = INNER.saturating_sub(visible);
    println!("{CYAN}{BOLD}│{RESET}{content}{}{CYAN}{BOLD}│{RESET}", " ".repeat(pad));

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
            BranchTag::Normal  => format!("{BLUE}{CIRCLE}{RESET}"),
        };

        let name_styled = match row.tag {
            BranchTag::Current => format!("{BOLD}{GREEN}{}{RESET}", row.name),
            BranchTag::Missing => format!("{DIM}{RED}{}{RESET}", row.name),
            BranchTag::Normal  => format!("{WHITE}{}{RESET}", row.name),
        };

        let tag_str = match row.tag {
            BranchTag::Current => format!("  {DIM}{GREEN}(current){RESET}"),
            BranchTag::Missing => format!("  {DIM}{RED}(missing){RESET}"),
            BranchTag::Normal  => String::new(),
        };

        println!("{CYAN}{connector}{RESET} {circle} {name_styled}{tag_str}");

        // Detail lines — printed verbatim with a fixed indent, no extra styling.
        for line in &row.detail {
            println!("     {line}");
        }

        // Vertical connector between items
        if i < rows.len() - 1 {
            println!("{CYAN}{VERTICAL}{RESET}");
        }
    }
}
