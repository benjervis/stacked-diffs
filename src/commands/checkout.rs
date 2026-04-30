use anyhow::Result;
use std::io::{self, IsTerminal, Read, Write};
use std::process::Command;

use crate::config::load_stack;
use crate::ctx::{branch_exists, git, git_ok, Ctx};
use crate::errors::{CmdError, CmdResult};
use crate::output::{
    err_print, ok, print_header, BLUE, BOLD, BG_CYAN, BLACK, BRANCH_END, BRANCH_MID, BRANCH_START,
    CIRCLE, CIRCLE_FILLED, CYAN, DIM, GREEN, RED, RESET, VERTICAL, WHITE, YELLOW,
};

/// Enhanced TUI selector with colors and tree structure.
/// Returns the index of the selected item, or None if aborted.
fn select_interactive(items: &[(String, &str)]) -> Option<usize> {
    use std::io::BufReader;

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = BufReader::new(stdin);

    // Enable raw mode for arrow key handling
    let _raw = enable_raw_mode();

    let mut selected = 0;
    loop {
        // Clear screen and redraw
        print!("\x1b[2J\x1b[H");

        // Header with styling
        println!("{CYAN}{BOLD}╭─────────────────────────────────────╮{RESET}");
        println!("{CYAN}{BOLD}│  {WHITE}Stack Branch Selector{CYAN}             │{RESET}");
        println!("{CYAN}{BOLD}╰─────────────────────────────────────╯{RESET}");
        println!();

        // Instructions with colors
        println!(
            "{DIM}Use {YELLOW}↑↓{DIM} or {YELLOW}j/k{DIM} to move, {GREEN}Enter{DIM} to select, {RED}Esc/q{DIM} to cancel:{RESET}"
        );
        println!();

        // Draw the tree structure
        for (i, (name, suffix)) in items.iter().enumerate() {
            let is_current = suffix.contains("current");
            let is_missing = suffix.contains("missing");

            // Tree connector
            let connector_char = if items.len() == 1 {
                BRANCH_END
            } else if i == 0 {
                BRANCH_START
            } else if i == items.len() - 1 {
                BRANCH_END
            } else {
                BRANCH_MID
            };
            let connector = format!("{CYAN}{connector_char}{RESET} ");

            // Selection indicator with background
            let selection = if i == selected {
                format!("{BG_CYAN}{BLACK}►{RESET} ")
            } else {
                "  ".to_string()
            };

            // Circle indicator
            let circle = if is_current {
                format!("{GREEN}{CIRCLE_FILLED}{RESET}")
            } else if is_missing {
                format!("{RED}{CIRCLE}{RESET}")
            } else {
                format!("{BLUE}{CIRCLE}{RESET}")
            };

            // Branch name with color
            let branch_name = if i == selected {
                format!("{BOLD}{WHITE}{BG_CYAN}{name}{RESET}")
            } else if is_current {
                format!("{BOLD}{GREEN}{name}")
            } else if is_missing {
                format!("{DIM}{RED}{name}")
            } else {
                format!("{WHITE}{name}")
            };

            // Suffix with styling
            let styled_suffix = if is_current {
                format!("{DIM}{GREEN}{suffix}{RESET}")
            } else if is_missing {
                format!("{DIM}{RED}{suffix}{RESET}")
            } else {
                suffix.to_string()
            };

            println!("{connector}{selection}{circle} {branch_name}");
            if !styled_suffix.is_empty() {
                println!("         {DIM}{styled_suffix}");
            }

            // Add connecting line for non-last items
            if i < items.len() - 1 {
                println!("{CYAN}{VERTICAL}{RESET}   ");
            }
        }

        stdout.flush().unwrap();

        // Read a single byte
        let mut buf = [0u8; 1];
        match reader.read(&mut buf) {
            Ok(1) => {
                match buf[0] {
                    b'\x1b' => {
                        // Escape sequence - try to read more bytes immediately
                        let mut seq_buf = [0u8; 2];
                        let mut total_read = 0;

                        // Try to read up to 2 more bytes without blocking too long
                        for i in 0..2 {
                            match reader.read(&mut seq_buf[i..i + 1]) {
                                Ok(1) => total_read += 1,
                                _ => break,
                            }
                        }

                        if total_read == 2 && seq_buf[0] == b'[' {
                            match seq_buf[1] {
                                b'A' => {
                                    // Up arrow
                                    if selected > 0 {
                                        selected -= 1;
                                    }
                                }
                                b'B' => {
                                    // Down arrow
                                    if selected < items.len() - 1 {
                                        selected += 1;
                                    }
                                }
                                _ => {} // Other arrow keys
                            }
                        } else {
                            // Just Esc or incomplete sequence
                            break None;
                        }
                    }
                    b'\r' | b'\n' => break Some(selected),
                    b'j' | b'J' => {
                        // j for down
                        if selected < items.len() - 1 {
                            selected += 1;
                        }
                    }
                    b'k' | b'K' => {
                        // k for up
                        if selected > 0 {
                            selected -= 1;
                        }
                    }
                    b'q' | b'Q' => break None,
                    _ => {
                        // Other keys, ignore
                    }
                }
            }
            Ok(0) | Ok(_) => break None, // EOF or unexpected read size
            Err(_) => break None,
        }
    }
}

/// Enable terminal raw mode for single-key input.
fn enable_raw_mode() -> impl Drop {
    // Save current settings and set raw mode with minimal flags
    let _ = Command::new("stty").args(["-echo", "-icanon"]).status();
    RawModeGuard
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Restore terminal mode
        let _ = Command::new("stty").args(["echo", "icanon"]).status();
    }
}

pub fn cmd_checkout(ctx: &Ctx, name: &str) -> Result<CmdResult> {
    let stack = load_stack(ctx, name)?;
    let all: Vec<String> = std::iter::once(stack.base.clone())
        .chain(stack.branches)
        .collect();

    if all.is_empty() {
        err_print(&format!("Stack '{name}' has no branches to checkout."));
        return Ok(Err(CmdError::UserError));
    }

    // Prepare items with suffixes
    let current_branch = git(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    let items: Vec<(String, &str)> = all
        .iter()
        .map(|branch| {
            let suffix = if current_branch.as_deref() == Some(branch.as_str()) {
                " (current)"
            } else if !branch_exists(ctx, branch) {
                " (missing)"
            } else {
                ""
            };
            (branch.clone(), suffix)
        })
        .collect();

    // Check if we're on a TTY for interactive mode
    let is_tty = io::stdin().is_terminal();

    println!("Stack: {name}");
    let selected = if is_tty {
        select_interactive(&items)
    } else {
        // Fallback to simple numbered selection for non-interactive environments
        select_simple(&items)
    };

    if let Some(selected) = selected {
        let branch = &all[selected];
        if !branch_exists(ctx, branch) {
            err_print(&format!("Branch '{}' does not exist locally.", branch));
            return Ok(Err(CmdError::UserError));
        }
        if git_ok(ctx, &["checkout", branch])? {
            ok(&format!("Checked out '{}'.", branch));
            Ok(Ok(()))
        } else {
            err_print(&format!("Failed to checkout '{}'.", branch));
            Ok(Err(CmdError::UserError))
        }
    } else {
        // User cancelled
        println!();
        Ok(Ok(()))
    }
}

/// Enhanced numbered selection fallback for non-TTY environments.
fn select_simple(items: &[(String, &str)]) -> Option<usize> {
    print_header("Select a branch to checkout:");
    println!();

    for (i, (name, suffix)) in items.iter().enumerate() {
        let is_current = suffix.contains("current");
        let is_missing = suffix.contains("missing");

        let number = if is_current {
            format!("{BOLD}{GREEN}{}.{RESET}", i + 1)
        } else {
            format!("{DIM}{}.", i + 1)
        };

        let circle = if is_current {
            format!("{GREEN}{CIRCLE_FILLED}{RESET}")
        } else if is_missing {
            format!("{RED}{CIRCLE}{RESET}")
        } else {
            format!("{BLUE}{CIRCLE}{RESET}")
        };

        let branch_name = if is_current {
            format!("{BOLD}{GREEN}{name}")
        } else if is_missing {
            format!("{DIM}{RED}{name}")
        } else {
            format!("{WHITE}{name}")
        };

        let styled_suffix = if is_current {
            format!(" {DIM}{GREEN}(current){RESET}")
        } else if is_missing {
            format!(" {DIM}{RED}(missing){RESET}")
        } else {
            String::new()
        };

        println!("  {number} {circle} {branch_name}{styled_suffix}");
    }

    println!();
    loop {
        eprint!("{CYAN}Enter number (1-{}): {YELLOW}{RESET}", items.len());
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => return None, // EOF
            Ok(_) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match trimmed.parse::<usize>() {
                    Ok(n) if 1 <= n && n <= items.len() => return Some(n - 1),
                    Ok(_) => {
                        eprintln!("{RED}Enter a number between 1 and {}.{RESET}", items.len());
                        continue;
                    }
                    Err(_) => {
                        eprintln!("{RED}Invalid input. Enter a number.{RESET}");
                        continue;
                    }
                }
            }
            Err(_) => return None,
        }
    }
}
