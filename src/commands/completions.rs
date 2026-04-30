use std::io;

use clap::CommandFactory;
use clap_complete::Shell;

use crate::cli::Cli;

/// Custom Fish completion snippet appended after clap's generated script.
///
/// `__sd_stacks` reads `.stacks/` relative to the git repo root (same logic
/// the binary uses at runtime) and returns one stack name per line.
///
/// The `complete` directives replace clap's generic positional argument
/// completions for every subcommand that takes a <stack> argument with
/// dynamic stack-name results instead.
const FISH_DYNAMIC_COMPLETIONS: &str = r#"
# --- sd dynamic stack-name completions ---

function __sd_stacks
    set -l root (git rev-parse --show-toplevel 2>/dev/null)
    or return
    for f in $root/.stacks/*
        if test -f $f
            basename $f
        end
    end
end

# Helper: true when the current token position is the first positional
# argument (i.e. the stack name slot) for each subcommand.
function __sd_needs_stack
    set -l cmd (commandline -opc)
    # cmd[1] is "sd", cmd[2] is the subcommand — we need exactly 2 tokens
    # before the current word (no stack yet).
    test (count $cmd) -eq 2
end

# Replace positional completions for every stack-taking subcommand.
for __sd_sub in rebase add rm show status push sync
    complete -c sd -n "__fish_seen_subcommand_from $__sd_sub; and __sd_needs_stack" \
        -f -a "(__sd_stacks)" -d "stack"
end
"#;

pub fn cmd_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, &bin_name, &mut io::stdout());
    if shell == Shell::Fish {
        print!("{}", FISH_DYNAMIC_COMPLETIONS);
    }
}
