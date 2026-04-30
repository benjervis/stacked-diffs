#!/usr/bin/env bash
# Integration tests for sd (stacked-diffs)
#
# Each test sets up a fresh git repo and exercises the binary end-to-end.
# The binary is expected to be on $PATH or passed via SD_BIN env var.
set -o pipefail

TEST_EXIT=0
OUT=""

# Locate the binary: SD_BIN env > cargo-built debug binary > PATH
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ -z "$SD_BIN" ]]; then
    # Try the cargo debug build first
    CARGO_BIN="$REPO_ROOT/target/debug/sd"
    if [[ -f "$CARGO_BIN" ]]; then
        SD_BIN="$CARGO_BIN"
    elif command -v sd >/dev/null 2>&1; then
        SD_BIN="sd"
    else
        echo "Cannot find 'sd' binary. Build with 'cargo build' or set SD_BIN." >&2
        exit 1
    fi
fi

WORKSPACE=$(mktemp -d)
trap 'rm -rf "$WORKSPACE"' EXIT

PASS=0
FAIL=0
FAILED_TESTS=()

color() { local c=$1; shift; printf '\033[%sm%s\033[0m' "$c" "$*"; }
red()   { color '1;31' "$*"; }
green() { color '1;32' "$*"; }
cyan()  { color '1;36' "$*"; }
gray()  { color '0;90' "$*"; }

assert_eq() {
    local got=$1 expected=$2 msg=$3
    if [[ "$got" == "$expected" ]]; then
        return 0
    fi
    echo "    $(red FAIL): $msg"
    echo "      expected: $expected"
    echo "      got:      $got"
    return 1
}

assert_contains() {
    local haystack=$1 needle=$2 msg=$3
    if [[ "$haystack" == *"$needle"* ]]; then
        return 0
    fi
    echo "    $(red FAIL): $msg"
    echo "      needle:   $needle"
    echo "      haystack: $haystack"
    return 1
}

assert_branch_exists() {
    local repo=$1 branch=$2
    if git -C "$repo" show-ref --verify --quiet "refs/heads/$branch"; then
        return 0
    fi
    echo "    $(red FAIL): expected branch '$branch' to exist"
    return 1
}

assert_branch_missing() {
    local repo=$1 branch=$2
    if git -C "$repo" show-ref --verify --quiet "refs/heads/$branch"; then
        echo "    $(red FAIL): expected branch '$branch' to NOT exist"
        return 1
    fi
    return 0
}

# Set up a fresh repo. The binary uses `git rev-parse --show-toplevel` to
# find the repo root at runtime, so no scripts/ symlink is needed.
new_repo() {
    local name=$1
    local repo="$WORKSPACE/$name"
    mkdir -p "$repo"
    git -C "$repo" -c init.defaultBranch=main init --quiet
    git -C "$repo" config user.email tester@example.com
    git -C "$repo" config user.name Tester
    echo "$repo"
}

# Run the binary from inside the repo directory so git rev-parse resolves correctly.
# Output is captured into the global $OUT; exit code into $TEST_EXIT.
run_rs() {
    local repo=$1; shift
    OUT=$(cd "$repo" && "$SD_BIN" "$@" 2>&1)
    TEST_EXIT=$?
}

# Set up a starter stack: main → feature-a → feature-b → feature-c, all linear.
setup_basic_stack() {
    local name=$1
    local repo
    repo=$(new_repo "$name")
    git -C "$repo" commit --quiet --allow-empty -m "initial main"
    echo "main-1" > "$repo/main.txt"
    git -C "$repo" add main.txt
    git -C "$repo" commit --quiet -m "main: add main.txt"

    git -C "$repo" checkout --quiet -b feature-a
    echo "a-1" > "$repo/a.txt"
    git -C "$repo" add a.txt
    git -C "$repo" commit --quiet -m "feature-a: add a.txt"

    git -C "$repo" checkout --quiet -b feature-b
    echo "b-1" > "$repo/b.txt"
    git -C "$repo" add b.txt
    git -C "$repo" commit --quiet -m "feature-b: add b.txt"

    git -C "$repo" checkout --quiet -b feature-c
    echo "c-1" > "$repo/c.txt"
    git -C "$repo" add c.txt
    git -C "$repo" commit --quiet -m "feature-c: add c.txt"

    mkdir -p "$repo/.stacks"
    cat > "$repo/.stacks/test-stack" <<EOF
# test stack
main
feature-a
feature-b
feature-c
EOF
    echo "$repo"
}

run_test() {
    local name=$1; shift
    local fn=$1
    echo
    echo "$(cyan "● $name")"
    if "$fn"; then
        echo "  $(green PASS)"
        PASS=$((PASS + 1))
    else
        echo "  $(red FAIL)"
        FAIL=$((FAIL + 1))
        FAILED_TESTS+=("$name")
    fi
}

# ---------------- Legacy rebase tests ----------------

test_clean_rebase() {
    local repo
    repo=$(setup_basic_stack repo1)
    git -C "$repo" checkout --quiet main
    echo "main-2" > "$repo/main2.txt"
    git -C "$repo" add main2.txt
    git -C "$repo" commit --quiet -m "main: extra commit after stack"

    local out
    run_rs "$repo" test-stack --no-fetch; out=$OUT
    assert_eq "$TEST_EXIT" 0 "rebase should succeed (exit 0)" || { echo "$out"; return 1; }

    for b in feature-a feature-b feature-c; do
        git -C "$repo" checkout --quiet "$b"
        [[ -f "$repo/main2.txt" ]] || { echo "    $(red FAIL): $b missing main2.txt"; return 1; }
    done
    git -C "$repo" checkout --quiet feature-c
    [[ -f "$repo/a.txt" && -f "$repo/b.txt" && -f "$repo/c.txt" ]] || { echo "    $(red FAIL): feature-c missing files"; return 1; }
    return 0
}

test_preserve_midstack_commits() {
    local repo
    repo=$(setup_basic_stack repo2)
    git -C "$repo" checkout --quiet main
    echo "main-2" > "$repo/main2.txt"
    git -C "$repo" add main2.txt
    git -C "$repo" commit --quiet -m "main: forward"
    git -C "$repo" checkout --quiet feature-a
    echo "a-extra" > "$repo/a-extra.txt"
    git -C "$repo" add a-extra.txt
    git -C "$repo" commit --quiet -m "feature-a: extra mid-stack commit"

    local out
    run_rs "$repo" test-stack --no-fetch; out=$OUT
    assert_eq "$TEST_EXIT" 0 "rebase should succeed" || { echo "$out"; return 1; }

    git -C "$repo" checkout --quiet feature-c
    for f in main2.txt a.txt a-extra.txt b.txt c.txt; do
        [[ -f "$repo/$f" ]] || { echo "    $(red FAIL): feature-c missing $f"; return 1; }
    done
    return 0
}

test_abort_after_conflict() {
    local repo
    repo=$(setup_basic_stack repo3)
    git -C "$repo" checkout --quiet main
    echo "from-main" > "$repo/conflict.txt"
    git -C "$repo" add conflict.txt
    git -C "$repo" commit --quiet -m "main: conflict.txt"
    git -C "$repo" checkout --quiet feature-a
    echo "from-a" > "$repo/conflict.txt"
    git -C "$repo" add conflict.txt
    git -C "$repo" commit --quiet -m "feature-a: conflicting conflict.txt"

    local pre_a pre_b pre_c
    pre_a=$(git -C "$repo" rev-parse feature-a)
    pre_b=$(git -C "$repo" rev-parse feature-b)
    pre_c=$(git -C "$repo" rev-parse feature-c)

    local out
    run_rs "$repo" test-stack --no-fetch; out=$OUT
    [[ "$TEST_EXIT" -eq 2 ]] || { echo "    $(red FAIL): expected exit 2 (conflict), got $TEST_EXIT"; echo "$out"; return 1; }

    run_rs "$repo" test-stack --abort; out=$OUT
    [[ "$TEST_EXIT" -eq 3 ]] || { echo "    $(red FAIL): expected exit 3 (aborted), got $TEST_EXIT"; echo "$out"; return 1; }

    local post_a post_b post_c
    post_a=$(git -C "$repo" rev-parse feature-a)
    post_b=$(git -C "$repo" rev-parse feature-b)
    post_c=$(git -C "$repo" rev-parse feature-c)
    assert_eq "$post_a" "$pre_a" "feature-a restored" || return 1
    assert_eq "$post_b" "$pre_b" "feature-b restored" || return 1
    assert_eq "$post_c" "$pre_c" "feature-c restored" || return 1
    return 0
}

test_resume_after_conflict() {
    local repo
    repo=$(setup_basic_stack repo4)
    git -C "$repo" checkout --quiet feature-a
    echo "shared-a" > "$repo/shared.txt"
    git -C "$repo" add shared.txt
    git -C "$repo" commit --quiet -m "feature-a: shared.txt"
    echo "feature-a extra" > "$repo/a-extra.txt"
    git -C "$repo" add a-extra.txt
    git -C "$repo" commit --quiet -m "feature-a: extra"

    git -C "$repo" checkout --quiet main
    echo "shared-main" > "$repo/shared.txt"
    git -C "$repo" add shared.txt
    git -C "$repo" commit --quiet -m "main: shared.txt"

    local out
    run_rs "$repo" test-stack --no-fetch; out=$OUT
    [[ "$TEST_EXIT" -eq 2 ]] || { echo "    $(red FAIL): expected exit 2, got $TEST_EXIT"; echo "$out"; return 1; }

    echo "shared-main" > "$repo/shared.txt"
    git -C "$repo" add shared.txt
    git -C "$repo" -c core.editor=true rebase --continue >/dev/null 2>&1 || true

    run_rs "$repo" test-stack --no-fetch; out=$OUT
    assert_eq "$TEST_EXIT" 0 "second run should succeed" || { echo "$out"; return 1; }

    git -C "$repo" checkout --quiet feature-c
    [[ -f "$repo/c.txt" ]] || { echo "    $(red FAIL): feature-c missing c.txt"; return 1; }
    return 0
}

test_dirty_tree_rejected() {
    local repo
    repo=$(setup_basic_stack repo5)
    git -C "$repo" checkout --quiet feature-c
    echo "uncommitted" >> "$repo/c.txt"
    local out
    run_rs "$repo" test-stack --no-fetch; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): expected nonzero exit"; return 1; }
    assert_contains "$out" "dirty" "should mention dirty tree" || return 1
    return 0
}

test_list_subcommand() {
    local repo
    repo=$(setup_basic_stack repo6)
    cat > "$repo/.stacks/another" <<EOF
main
feature-a
EOF
    local out
    run_rs "$repo" --list; out=$OUT
    assert_eq "$TEST_EXIT" 0 "--list should succeed" || { echo "$out"; return 1; }
    assert_contains "$out" "test-stack" "should list test-stack" || return 1
    assert_contains "$out" "another" "should list another" || return 1
    return 0
}

test_unknown_flag_rejected() {
    local repo
    repo=$(setup_basic_stack repo7)
    local out
    run_rs "$repo" test-stack --bogus; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): expected error"; return 1; }
    assert_contains "$out" "Unknown flag" "should mention unknown flag" || return 1
    return 0
}

test_slash_branch_names() {
    local repo
    repo=$(new_repo repo8)
    git -C "$repo" commit --quiet --allow-empty -m initial
    git -C "$repo" checkout --quiet -b ben/feat-a
    echo a > "$repo/a"
    git -C "$repo" add a
    git -C "$repo" commit --quiet -m a
    git -C "$repo" checkout --quiet -b ben/feat-b
    echo b > "$repo/b"
    git -C "$repo" add b
    git -C "$repo" commit --quiet -m b

    mkdir -p "$repo/.stacks"
    cat > "$repo/.stacks/slashy" <<EOF
main
ben/feat-a
ben/feat-b
EOF
    git -C "$repo" checkout --quiet main
    echo m > "$repo/m"
    git -C "$repo" add m
    git -C "$repo" commit --quiet -m m

    local out
    run_rs "$repo" slashy --no-fetch; out=$OUT
    assert_eq "$TEST_EXIT" 0 "slash branch rebase should succeed" || { echo "$out"; return 1; }
    git -C "$repo" checkout --quiet ben/feat-b
    [[ -f "$repo/m" && -f "$repo/a" && -f "$repo/b" ]] || { echo "    $(red FAIL): missing files"; return 1; }
    return 0
}

# ---------------- New subcommand tests ----------------

test_init_creates_config() {
    local repo
    repo=$(new_repo repo_init)
    git -C "$repo" commit --quiet --allow-empty -m initial
    local out
    run_rs "$repo" init my-stack; out=$OUT
    assert_eq "$TEST_EXIT" 0 "init should succeed" || { echo "$out"; return 1; }
    [[ -f "$repo/.stacks/my-stack" ]] || { echo "    $(red FAIL): config not created"; return 1; }
    grep -q '^main$' "$repo/.stacks/my-stack" || { echo "    $(red FAIL): base 'main' not in config"; return 1; }

    run_rs "$repo" init my-stack; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): re-init should fail"; return 1; }
    assert_contains "$out" "already exists" "should mention already exists" || return 1
    return 0
}

test_init_with_explicit_base() {
    local repo
    repo=$(new_repo repo_init_base)
    git -C "$repo" commit --quiet --allow-empty -m initial
    git -C "$repo" checkout --quiet -b develop
    git -C "$repo" checkout --quiet main

    local out
    run_rs "$repo" init dev-stack --base develop; out=$OUT
    assert_eq "$TEST_EXIT" 0 "init should succeed with custom base" || { echo "$out"; return 1; }
    grep -q '^develop$' "$repo/.stacks/dev-stack" || { echo "    $(red FAIL): custom base not in config"; return 1; }

    run_rs "$repo" init bad-stack --base does-not-exist; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): expected error for missing base"; return 1; }
    assert_contains "$out" "does not exist" "should reject missing base" || return 1
    return 0
}

test_add_creates_branch() {
    local repo
    repo=$(new_repo repo_add)
    git -C "$repo" commit --quiet --allow-empty -m initial
    run_rs "$repo" init s >/dev/null
    [[ "$TEST_EXIT" -eq 0 ]] || { echo "init failed"; return 1; }

    local out
    run_rs "$repo" add s alpha; out=$OUT
    assert_eq "$TEST_EXIT" 0 "add should succeed" || { echo "$out"; return 1; }
    assert_branch_exists "$repo" alpha || return 1
    grep -q '^alpha$' "$repo/.stacks/s" || { echo "    $(red FAIL): alpha not in config"; return 1; }

    local main_tip alpha_tip
    main_tip=$(git -C "$repo" rev-parse main)
    alpha_tip=$(git -C "$repo" rev-parse alpha)
    assert_eq "$alpha_tip" "$main_tip" "alpha should fork from main" || return 1

    git -C "$repo" checkout --quiet alpha
    echo "a" > "$repo/a.txt"
    git -C "$repo" add a.txt
    git -C "$repo" commit --quiet -m "alpha: a.txt"
    local alpha_tip
    alpha_tip=$(git -C "$repo" rev-parse alpha)

    run_rs "$repo" add s beta; out=$OUT
    assert_eq "$TEST_EXIT" 0 "add beta should succeed" || { echo "$out"; return 1; }
    local beta_tip
    beta_tip=$(git -C "$repo" rev-parse beta)
    assert_eq "$beta_tip" "$alpha_tip" "beta should fork from alpha tip" || return 1

    local lines
    lines=$(grep -v '^#' "$repo/.stacks/s" | grep -v '^$' | tr '\n' ' ')
    assert_eq "$lines" "main alpha beta " "config order" || return 1
    return 0
}

test_add_rejects_existing_branch() {
    local repo
    repo=$(new_repo repo_add_dup)
    git -C "$repo" commit --quiet --allow-empty -m initial
    git -C "$repo" branch already-here
    run_rs "$repo" init s >/dev/null

    local out
    run_rs "$repo" add s already-here; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): should refuse existing branch"; return 1; }
    assert_contains "$out" "already exists" "should mention already exists" || return 1
    return 0
}

test_add_rejects_dirty_tree() {
    local repo
    repo=$(new_repo repo_add_dirty)
    git -C "$repo" commit --quiet --allow-empty -m initial
    run_rs "$repo" init s >/dev/null

    echo "junk" > "$repo/dirty.txt"
    git -C "$repo" add dirty.txt
    local out
    run_rs "$repo" add s alpha; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): should reject dirty"; return 1; }
    assert_contains "$out" "dirty" "should mention dirty" || return 1
    return 0
}

test_show_subcommand() {
    local repo
    repo=$(setup_basic_stack repo_show)
    local out
    run_rs "$repo" show test-stack; out=$OUT
    assert_eq "$TEST_EXIT" 0 "show should succeed" || { echo "$out"; return 1; }
    assert_contains "$out" "Stack: test-stack" "should print stack name" || return 1
    assert_contains "$out" "base: main" "should print base" || return 1
    assert_contains "$out" "feature-a -> feature-b -> feature-c" "should print chain" || return 1
    return 0
}

test_status_subcommand() {
    local repo
    repo=$(setup_basic_stack repo_status)
    local out
    run_rs "$repo" status test-stack; out=$OUT
    assert_eq "$TEST_EXIT" 0 "status should succeed" || { echo "$out"; return 1; }
    assert_contains "$out" "Stack: test-stack" "should print header" || return 1
    assert_contains "$out" "feature-a" "should list feature-a" || return 1
    assert_contains "$out" "feature-c" "should list feature-c" || return 1
    assert_contains "$out" "ahead 1 of feature-b" "feature-c should be ahead 1 of feature-b" || return 1

    git -C "$repo" checkout --quiet main
    echo "moved" > "$repo/moved.txt"
    git -C "$repo" add moved.txt
    git -C "$repo" commit --quiet -m "main: moved"
    run_rs "$repo" status test-stack; out=$OUT
    assert_contains "$out" "behind 1 (rebase needed)" "should show behind 1 after main moves" || return 1
    return 0
}

test_rm_subcommand() {
    local repo
    repo=$(setup_basic_stack repo_rm)
    local out
    run_rs "$repo" rm test-stack feature-c; out=$OUT
    assert_eq "$TEST_EXIT" 0 "rm should succeed" || { echo "$out"; return 1; }
    grep -q '^feature-c$' "$repo/.stacks/test-stack" && { echo "    $(red FAIL): feature-c still in config"; return 1; }
    grep -q '^feature-b$' "$repo/.stacks/test-stack" || { echo "    $(red FAIL): feature-b lost"; return 1; }
    assert_branch_exists "$repo" feature-c || return 1

    run_rs "$repo" rm test-stack does-not-exist; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): rm of non-stack branch should fail"; return 1; }

    run_rs "$repo" rm test-stack feature-a; out=$OUT
    assert_eq "$TEST_EXIT" 0 "mid-stack rm should succeed" || { echo "$out"; return 1; }
    assert_contains "$out" "not at the top" "should warn about mid-stack" || return 1
    return 0
}

test_rebase_subcommand_explicit() {
    local repo
    repo=$(setup_basic_stack repo_rebase_explicit)
    git -C "$repo" checkout --quiet main
    echo "main-2" > "$repo/m2.txt"
    git -C "$repo" add m2.txt
    git -C "$repo" commit --quiet -m m2

    local out
    run_rs "$repo" rebase test-stack --no-fetch; out=$OUT
    assert_eq "$TEST_EXIT" 0 "explicit rebase should succeed" || { echo "$out"; return 1; }
    git -C "$repo" checkout --quiet feature-c
    [[ -f "$repo/m2.txt" ]] || { echo "    $(red FAIL): missing m2.txt"; return 1; }
    return 0
}

test_full_workflow() {
    local repo
    repo=$(new_repo repo_workflow)
    git -C "$repo" commit --quiet --allow-empty -m initial
    echo "main-1" > "$repo/m1"
    git -C "$repo" add m1
    git -C "$repo" commit --quiet -m m1

    run_rs "$repo" init flow >/dev/null
    [[ "$TEST_EXIT" -eq 0 ]] || { echo "init failed"; return 1; }

    run_rs "$repo" add flow alpha >/dev/null
    [[ "$TEST_EXIT" -eq 0 ]] || { echo "add alpha failed"; return 1; }
    git -C "$repo" checkout --quiet alpha
    echo a > "$repo/a"; git -C "$repo" add a; git -C "$repo" commit --quiet -m a

    run_rs "$repo" add flow beta >/dev/null
    [[ "$TEST_EXIT" -eq 0 ]] || { echo "add beta failed"; return 1; }
    git -C "$repo" checkout --quiet beta
    echo b > "$repo/b"; git -C "$repo" add b; git -C "$repo" commit --quiet -m b

    run_rs "$repo" add flow gamma >/dev/null
    [[ "$TEST_EXIT" -eq 0 ]] || { echo "add gamma failed"; return 1; }
    git -C "$repo" checkout --quiet gamma
    echo g > "$repo/g"; git -C "$repo" add g; git -C "$repo" commit --quiet -m g

    git -C "$repo" checkout --quiet main
    echo m2 > "$repo/m2"; git -C "$repo" add m2; git -C "$repo" commit --quiet -m m2

    local out
    run_rs "$repo" rebase flow --no-fetch; out=$OUT
    assert_eq "$TEST_EXIT" 0 "workflow rebase should succeed" || { echo "$out"; return 1; }
    git -C "$repo" checkout --quiet gamma
    for f in m1 m2 a b g; do
        [[ -f "$repo/$f" ]] || { echo "    $(red FAIL): gamma missing $f"; return 1; }
    done

    run_rs "$repo" show flow; out=$OUT
    assert_contains "$out" "alpha -> beta -> gamma" "should show full chain" || return 1
    return 0
}

# ---------------- --scan tests ----------------

test_scan_detects_stack() {
    # Set up main → feature-a → feature-b → feature-c, checked out on feature-c
    local repo
    repo=$(setup_basic_stack repo_scan)
    # HEAD is on feature-c after setup_basic_stack
    local out
    run_rs "$repo" init my-stack --scan; out=$OUT
    assert_eq "$TEST_EXIT" 0 "init --scan should succeed" || { echo "$out"; return 1; }
    [[ -f "$repo/.stacks/my-stack" ]] || { echo "    $(red FAIL): config not created"; return 1; }
    # Config should contain all three feature branches in order
    local lines
    lines=$(grep -v '^#' "$repo/.stacks/my-stack" | grep -v '^$' | tr '\n' ' ')
    assert_eq "$lines" "main feature-a feature-b feature-c " "config should have branches in order" || return 1
    return 0
}

test_scan_on_base_warns() {
    local repo
    repo=$(new_repo repo_scan_base)
    git -C "$repo" commit --quiet --allow-empty -m initial
    # HEAD is on main
    local out
    run_rs "$repo" init my-stack --scan; out=$OUT
    assert_eq "$TEST_EXIT" 0 "init --scan on base should succeed" || { echo "$out"; return 1; }
    [[ -f "$repo/.stacks/my-stack" ]] || { echo "    $(red FAIL): config not created"; return 1; }
    assert_contains "$out" "no branches detected" "should warn about no branches" || return 1
    # Config should have only the base
    local lines
    lines=$(grep -v '^#' "$repo/.stacks/my-stack" | grep -v '^$' | tr '\n' ' ')
    assert_eq "$lines" "main " "config should have only base" || return 1
    return 0
}

test_scan_detached_head_errors() {
    local repo
    repo=$(setup_basic_stack repo_scan_detached)
    # Detach HEAD
    local sha
    sha=$(git -C "$repo" rev-parse feature-c)
    git -C "$repo" checkout --quiet --detach "$sha"
    local out
    run_rs "$repo" init my-stack --scan; out=$OUT
    [[ "$TEST_EXIT" -ne 0 ]] || { echo "    $(red FAIL): expected error for detached HEAD"; return 1; }
    assert_contains "$out" "detached" "should mention detached HEAD" || return 1
    return 0
}

test_checkout_interactive() {
    local repo
    repo=$(setup_basic_stack checkout1)

    # Checkout base (option 1)
    git -C "$repo" checkout --quiet main
    run_rs "$repo" checkout test-stack <<< "1"
    if [[ "$OUT" != *"Checked out 'main'."* ]]; then
        echo "    $(red FAIL): checkout base branch"
        return 1
    fi

    # Checkout feature-c (option 4)
    run_rs "$repo" checkout test-stack <<< "4"
    if [[ "$OUT" != *"Checked out 'feature-c'."* ]]; then
        echo "    $(red FAIL): checkout top branch"
        return 1
    fi

    # Verify we're on feature-c
    local current
    current=$(git -C "$repo" rev-parse --abbrev-ref HEAD)
    if [[ "$current" != "feature-c" ]]; then
        echo "    $(red FAIL): expected to be on feature-c, got $current"
        return 1
    fi

    # Invalid input handling
    OUT=$(cd "$repo" && printf "x\n2\n" | "$SD_BIN" checkout test-stack 2>&1)
    if [[ "$OUT" != *"Invalid input. Enter a number."* ]]; then
        echo "    $(red FAIL): checkout should reject non-numeric input"
        return 1
    fi
    return 0
}

# ---------------- Run them ----------------

run_test "rebase: clean three-branch stack with main moved"      test_clean_rebase
run_test "rebase: preserves new commits on mid-stack branch"     test_preserve_midstack_commits
run_test "rebase: --abort restores branches after conflict"      test_abort_after_conflict
run_test "rebase: resumes after manual conflict resolution"      test_resume_after_conflict
run_test "rebase: rejects dirty working tree"                    test_dirty_tree_rejected
run_test "--list: prints all configured stacks"                  test_list_subcommand
run_test "rebase: rejects unknown flags"                         test_unknown_flag_rejected
run_test "rebase: handles slash-named branches"                  test_slash_branch_names
run_test "init: creates config; rejects re-init"                 test_init_creates_config
run_test "init: accepts custom base; rejects missing base"       test_init_with_explicit_base
run_test "add: creates branch off top of stack"                  test_add_creates_branch
run_test "add: rejects existing branch name"                     test_add_rejects_existing_branch
run_test "add: rejects dirty tree"                               test_add_rejects_dirty_tree
run_test "show: prints stack chain"                              test_show_subcommand
run_test "status: shows ahead/behind state"                      test_status_subcommand
run_test "rm: removes from config but leaves branch"             test_rm_subcommand
run_test "rebase: works with explicit 'rebase' subcommand"       test_rebase_subcommand_explicit
run_test "workflow: init → add×3 → main moves → rebase → show"   test_full_workflow
run_test "init --scan: detects stack from HEAD ancestry"          test_scan_detects_stack
run_test "init --scan: warns when HEAD is already on base"        test_scan_on_base_warns
run_test "init --scan: errors on detached HEAD"                   test_scan_detached_head_errors
run_test "checkout: interactive selection and checkout"            test_checkout_interactive

echo
echo "============================================"
echo "  $(green "Passed: $PASS")    $([[ $FAIL -gt 0 ]] && red "Failed: $FAIL" || echo "Failed: 0")"
echo "============================================"
if [[ $FAIL -gt 0 ]]; then
    echo
    for t in "${FAILED_TESTS[@]}"; do
        echo "  $(red "✗") $t"
    done
    exit 1
fi
