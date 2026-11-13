#!/bin/sh
# Run from this isolated checkout. Local-only: fixtures and GPU export, no daemon.
set -eu
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
OUT=${OUT:-apps/rndr-home/docs/desktop-evidence}
TARGET=${TARGET:-.rndr-target}
mkdir -p "$OUT"
: > "$OUT/results.tsv"
run() {
    name=$1
    shift
    printf '%s\n' "$*" > "$OUT/$name.command.txt"
    if "$@" > "$OUT/$name.log" 2>&1; then
        printf '%s\t0\n' "$name" >> "$OUT/results.tsv"
    else
        status=$?
        printf '%s\t%s\n' "$name" "$status" >> "$OUT/results.tsv"
        tail -40 "$OUT/$name.log"
        return "$status"
    fi
}
{
    date -u
    git rev-parse HEAD
    git diff --name-only
    rustc --version
    cargo --version
    df -h .
} > "$OUT/environment.txt"
run fmt cargo fmt --manifest-path apps/rndr-home/Cargo.toml --check
run test-default cargo test --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2
run test-observe cargo test --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2 --features observe
run clippy-default cargo clippy --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2 --all-targets -- -D warnings
run clippy-observe cargo clippy --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2 --all-targets --features observe -- -D warnings
run build-default cargo build --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2
run build-observe cargo build --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2 --features observe
run render cargo run --manifest-path apps/rndr-home/Cargo.toml --target-dir "$TARGET" -j 2 -- --export-desktop "$OUT/renders"
du -sh "$TARGET" "$OUT" > "$OUT/disk.txt"
# Do not silently equate a repository-wide docs gate with app verification.
if bash scripts/verify-docs.sh --strict > "$OUT/docs-strict.log" 2>&1; then
    printf 'docs-strict\t0\n' >> "$OUT/results.tsv"
else
    printf 'docs-strict\t%s\n' "$?" >> "$OUT/results.tsv"
fi
