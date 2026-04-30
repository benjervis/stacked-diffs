# stacked-diffs

A small Rust CLI (`sd`) for keeping a chain of dependent git branches rebased onto each other — useful during code freezes or when stacking PRs.

```
    o <- branch: feature-c       (PR into feature-b)
    |
    o <- branch: feature-b       (PR into feature-a)
    |
    o <- branch: feature-a       (PR into main)
    |
    o <- main
```

When `main` (or any mid-stack branch) gets a new commit, run `sd` and it walks the chain bottom-to-top, rebasing each branch onto its parent.

## Installation

```sh
cargo install --git https://github.com/benjervis/stacked-diffs
```

Or clone and build locally:

```sh
git clone https://github.com/benjervis/stacked-diffs
cd stacked-diffs
cargo build --release
# Add target/release/sd to your $PATH
```

## Stack config format

One file per stack at `.stacks/<stack-name>` (no extension required) in your project repo:

- First non-comment line: the **base** branch (e.g. `main`).
- Remaining non-comment lines: the **stack branches**, listed bottom-to-top.
- Lines starting with `#` and blank lines are ignored.

Example `.stacks/code-freeze`:

```
# Stack while merges are blocked
main
ben/feature-a
ben/feature-b
ben/feature-c
```

These files are intended to be checked in — they describe an intent, like a `.gitignore` or `.editorconfig`, and recover automatically across machines.

## Commands

| Command                             | Purpose                                                                                                                                                           |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sd init <stack> [--base <base>] [--scan]` | Create a new stack config. Base defaults to `origin/HEAD` (falls back to `main`). `--scan` walks ancestry from HEAD to base and populates branches automatically. |
| `sd add <stack> <branch>`           | `git checkout -b <branch>` from the top of the stack and append to config.                                                                                        |
| `sd rm <stack> <branch>`            | Remove a branch from the config (does NOT delete the local branch).                                                                                               |
| `sd show <stack>`                   | Print the stack chain in one line.                                                                                                                                |
| `sd status <stack> [--remote NAME]` | Per-branch tip + ahead/behind vs parent + remote sync state.                                                                                                      |
| `sd rebase <stack> [flags]`         | Rebase every branch in the stack onto its parent. Flags: `--no-fetch`, `--remote NAME`, `--abort`.                                                                |
| `sd push <stack> [--remote NAME]`   | `git push --force-with-lease` each branch.                                                                                                                        |
| `sd --list`                         | List all configured stacks.                                                                                                                                       |
| `sd --help`                         | Show full help.                                                                                                                                                   |

Bare `sd <stack>` is shorthand for `sd rebase <stack>`.

## Typical workflow

```sh
# 1a. Starting fresh — create the config and add branches one at a time.
sd init code-freeze
sd add code-freeze ben/feature-a
# work on feature-a, commit
sd add code-freeze ben/feature-b
# work on feature-b, commit
sd add code-freeze ben/feature-c
# work on feature-c, commit

# 1b. Already have a stack of branches? Check out the top branch and scan.
#     sd detects the base via origin/HEAD and walks ancestry back to it.
git checkout ben/feature-c
sd init code-freeze --scan
# → Detected 3 branches: ben/feature-a -> ben/feature-b -> ben/feature-c

# Override the base if needed:
sd init code-freeze --base master --scan

# 2. Push all branches up so PRs can be opened.
sd push code-freeze

# 3. main moves, or you commit directly to a mid-stack branch.
#    Rebase the whole chain back into a single line:
sd rebase code-freeze
# (or just: sd code-freeze)

# 4. Force-push the rebased branches.
sd push code-freeze

# At any point, see where each branch sits:
sd status code-freeze
```

## Conflicts

If a rebase hits a conflict mid-stack, `sd` stops and leaves git in the normal mid-rebase state. Resolve as you would any rebase:

```sh
# fix files, then
git add <files>
git rebase --continue
```

…then re-run `sd rebase <stack>`. It picks up where it left off, skipping the branch you just finished.

To bail out entirely and restore every branch to the SHA it had before the run started:

```sh
sd rebase <stack> --abort
```

## How it preserves mid-stack commits

For each branch `sd` runs:

```
git rebase --onto <new-parent-tip> <old-parent-tip> <branch>
```

`<old-parent-tip>` is snapshotted at the start of the run, _before_ any rebase happens. That means commits added directly to a mid-stack branch (e.g. you committed to `feature-a` while `feature-b` is still pointing at an older `feature-a` tip) get pulled forward into child branches automatically, without duplication.

## Exit codes

| Code | Meaning                                |
| ---- | -------------------------------------- |
| 0    | Success                                |
| 1    | Invocation/config error                |
| 2    | Rebase conflict — user action required |
| 3    | Rebase aborted                         |

## License

MIT
