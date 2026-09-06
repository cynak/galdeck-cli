#!/usr/bin/env bash
#
# Architectural invariants that are cheaper to check than to remember.
#
# This repository is the floor of the galdeck stack: a CLI, the control
# protocol it speaks, and the configuration model that protocol carries. The
# daemon and its configuration UI live in ../galdeck-daemon and depend on
# these crates. Nothing here may depend on anything there -- that is the whole
# point of the split, and it is one careless `path = "../galdeck-daemon/..."`
# away from being untrue, so it is checked rather than remembered.
set -uo pipefail

fail=0
note() { printf '  %s\n' "$*"; }

# Lines of real code, with comments and doc comments stripped out.
code_grep() {
    local pattern="$1"; shift
    grep -rnE "$pattern" "$@" --include='*.rs' 2>/dev/null | grep -vE ':[[:space:]]*//'
}

no_match() {
    local out
    out=$(code_grep "$@")
    if [ -n "$out" ]; then
        printf 'FAIL\n'
        printf '%s\n' "$out" | sed 's/^/    /'
        return 1
    fi
    return 0
}

PURE=(crates/galdeck-model/src)

printf '\nlayering\n'

# The pure crate is pure so its tests can name every instant. A sixty second
# soak that takes sixty seconds is a soak nobody runs.
printf '%-58s' "no wall clock in the pure crate"
no_match '(Instant|SystemTime)::now\(\)' "${PURE[@]}" && printf 'ok\n' || fail=1

# Ambient authority in the pure crate would make it untestable in the same
# way. This targets spawning and sockets specifically rather than the whole
# `std::process` module: reading our own pid is neither, and a staging filename
# legitimately wants it.
printf '%-58s' "no subprocesses or sockets in the pure crate"
no_match 'std::process::(Command|exit|abort)|std::net::' "${PURE[@]}" && printf 'ok\n' || fail=1

# `galdeck detect` deliberately bypasses the daemon and opens the device
# passively, so it is the one place that may name the concrete device type.
printf '%-58s' "only galdeck detect names Galleon"
offenders=$(code_grep 'Galleon' crates/*/src \
    | grep -v '^crates/galdeck-cli/src/main.rs:')
if [ -n "$offenders" ]; then
    printf 'FAIL\n'; printf '%s\n' "$offenders" | sed 's/^/    /'; fail=1
else
    printf 'ok\n'
fi

# The dependency runs ipc -> model, never the reverse: the protocol carries
# diagnostics and patches, so the model has to be the floor of the graph.
printf '%-58s' "galdeck-model does not depend on galdeck-ipc"
if grep -q 'galdeck-ipc' crates/galdeck-model/Cargo.toml 2>/dev/null; then
    printf 'FAIL\n'; note 'galdeck-model/Cargo.toml names galdeck-ipc'; fail=1
else
    printf 'ok\n'
fi

# The one that earns the two repositories. A CLI that reached for the daemon's
# crates would still build, and would quietly make this side unusable without
# the UI -- exactly backwards.
printf '%-58s' "nothing here depends on the daemon or the UI"
back=$(grep -rnE 'galdeck-(daemon|core|device|http|plugin)' Cargo.toml crates/*/Cargo.toml 2>/dev/null)
if [ -n "$back" ]; then
    printf 'FAIL\n'; printf '%s\n' "$back" | sed 's/^/    /'; fail=1
else
    printf 'ok\n'
fi

# The same rule in the source, where it would arrive as a `use` before anyone
# thought to add the dependency.
printf '%-58s' "no source here names a daemon-side crate"
no_match 'galdeck_(daemon|core|device|http|plugin)' crates/*/src crates/*/tests \
    && printf 'ok\n' || fail=1

# These two are published, and the daemon consumes them from crates.io. A
# bare path builds perfectly well here and then fails at `cargo publish`,
# which strips `path` and refuses a dependency with no version left. CI cannot
# catch that with `cargo package` until galdeck-model is on the index -- the
# packaging step resolves against the real registry -- so it is checked here.
for shared in galdeck-ipc galdeck-model; do
    printf '%-58s' "$shared is declared with a version and a path"
    line=$(grep -E "^$shared = " Cargo.toml)
    if [ -z "$line" ]; then
        printf 'FAIL\n'; note "Cargo.toml does not declare $shared"; fail=1
    elif ! printf '%s' "$line" | grep -q 'version *='; then
        printf 'FAIL\n'; note "no version, so it cannot be published: $line"; fail=1
    else
        printf 'ok\n'
    fi
done

# A test that reads a file from outside its own crate passes here, where the
# workspace root is two levels up, and fails for anyone who unpacks the
# published tarball -- which is where the daemon's copy comes from.
printf '%-58s' "no crate reaches outside itself for a file"
escapes=$(grep -rnE 'include_(str|bytes)!\("\.\./\.\./' crates/*/src crates/*/tests --include='*.rs' 2>/dev/null)
if [ -n "$escapes" ]; then
    printf 'FAIL\n'; printf '%s\n' "$escapes" | sed 's/^/    /'; fail=1
else
    printf 'ok\n'
fi

printf '\n'
if [ "$fail" -ne 0 ]; then
    echo 'layering check failed'
    exit 1
fi
echo 'layering ok'
