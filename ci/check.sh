#!/usr/bin/env bash
#
# Everything CI runs, in the order CI runs it, before you push.
#
#   ci/check.sh          fmt, clippy, tests, MSRV, layering, packaging
#   ci/check.sh fast     fmt, clippy, tests only -- the inner-loop subset
#   ci/check.sh msrv     just the 1.88 build
#
# CI is the authority; this is a local mirror of .github/workflows/ci.yml. If
# the two ever disagree, the workflow is right and this file is stale.
set -uo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

MSRV="$(grep -m1 '^rust-version' Cargo.toml | cut -d'"' -f2)"

bold() { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$*"; }
bad()  { printf '  \033[31m✗\033[0m %s\n' "$*"; }

failed=()
step() {
    local name="$1"; shift
    bold "$name"
    if "$@"; then
        ok "$name"
    else
        bad "$name"
        failed+=("$name")
    fi
}

# ---- which framework are we building against? ------------------------------
#
# .cargo/config.toml can patch `galdeck` to the checkout beside this one. That
# is the whole point of the patch, but it changes what a green run means, so
# it is stated up front rather than left to be discovered.
#
# It also costs the lockfile its source and checksum for that one package,
# which is why --locked is dropped while the patch is live: CI's lock and this
# one are legitimately different, and refusing to build would just be noise.
PATCHED=0
if [ -f .cargo/config.toml ] && grep -q '^\[patch\.crates-io\]' .cargo/config.toml; then
    PATCHED=1
fi

LOCKED=(--locked)
if [ "$PATCHED" = "1" ]; then
    LOCKED=()
    printf '\033[1mframework\033[0m  local: %s\n' "$(sed -n 's/.*path *= *"\([^"]*\)".*/\1/p' .cargo/config.toml | head -1)"
    warn "building against a patched galdeck -- CI builds the published one"
    warn "Cargo.lock is modified as a side effect; do not commit it"
else
    printf '\033[1mframework\033[0m  crates.io, as CI builds it\n'
fi

# ---- the steps -------------------------------------------------------------

run_fmt()      { cargo fmt --all --check; }
run_clippy()   { cargo clippy --workspace --all-targets "${LOCKED[@]}" -- -D warnings; }
run_test()     { cargo test --workspace "${LOCKED[@]}"; }
run_layering() { ./ci/layering.sh; }

run_msrv() {
    # The floor the workspace advertises. A dependency bump is what silently
    # raises it, so this job is the canary rather than a formality.
    if ! rustup toolchain list 2>/dev/null | grep -q "^$MSRV"; then
        warn "toolchain $MSRV not installed -- rustup toolchain install $MSRV"
        return 0
    fi
    cargo "+$MSRV" check --workspace --all-targets "${LOCKED[@]}"
}

run_package() {
    # The daemon consumes these crates from crates.io, so a change that makes
    # them unpublishable breaks the other repository, a long way from the edit
    # that caused it.
    if [ "$PATCHED" = "1" ]; then
        warn "skipped: cargo package resolves against the real index, and the"
        warn "patched framework is not on it. Comment the patch out to run it."
        return 0
    fi
    cargo package -p galdeck-model --locked
}

case "${1:-all}" in
    fast)
        step "fmt"      run_fmt
        step "clippy"   run_clippy
        step "test"     run_test
        ;;
    msrv)
        step "msrv ($MSRV)" run_msrv
        ;;
    all)
        step "fmt"          run_fmt
        step "clippy"       run_clippy
        step "test"         run_test
        step "msrv ($MSRV)" run_msrv
        step "layering"     run_layering
        step "packaging"    run_package
        ;;
    -h|--help|help)
        sed -n '3,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *)
        bad "unknown target: $1"; exit 1 ;;
esac

# ---- the lockfile guard ----------------------------------------------------
#
# A lock with the patch baked into it builds fine here and fails every CI job
# with --locked. Cheaper to catch on the way out than in a red build.
if [ "$PATCHED" = "1" ] && ! git diff --quiet -- Cargo.lock 2>/dev/null; then
    bold "lockfile"
    warn "Cargo.lock carries the patch. Before committing:"
    warn "    git checkout Cargo.lock"
fi

bold "summary"
if [ ${#failed[@]} -ne 0 ]; then
    for f in "${failed[@]}"; do bad "$f"; done
    exit 1
fi
ok "all checks passed"
