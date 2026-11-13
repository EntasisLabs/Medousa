#!/bin/sh
# Run from repository root. No daemon, credential, provider or admin operations.
set -u
D=apps/rndr-home/docs/shell-activity-evidence
mkdir -p "$D"
: > "$D/results.tsv"
run() {
  name=$1; shift
  first=1
  : > "$D/$name.command.txt"
  for argument in "$@"; do
    if [ "$first" = 1 ]; then first=0; else printf ' ' >> "$D/$name.command.txt"; fi
    printf '%s' "$argument" >> "$D/$name.command.txt"
  done
  printf '\n' >> "$D/$name.command.txt"
  "$@" > "$D/$name.log" 2>&1
  status=$?
  printf '%s\t%s\n' "$name" "$status" >> "$D/results.tsv"
  return "$status"
}
run fmt cargo fmt --manifest-path apps/rndr-home/Cargo.toml --check || exit $?
run test-default cargo test --manifest-path apps/rndr-home/Cargo.toml -j 2 || exit $?
run test-observe cargo test --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2 || exit $?
run build-default cargo build --manifest-path apps/rndr-home/Cargo.toml -j 2 || exit $?
run build-observe cargo build --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2 || exit $?
run clippy-default cargo clippy --manifest-path apps/rndr-home/Cargo.toml --all-targets -j 2 -- -D warnings || exit $?
run clippy-observe cargo clippy --manifest-path apps/rndr-home/Cargo.toml --all-targets --features observe -j 2 -- -D warnings || exit $?
run render cargo run --manifest-path apps/rndr-home/Cargo.toml --features observe -j 2 -- --export-desktop "$D/renders" || exit $?
printf 'source-head=%s\n' "$(git rev-parse HEAD)" > "$D/environment.txt"
date -u '+utc=%Y-%m-%dT%H:%M:%SZ' >> "$D/environment.txt"
rustc --version >> "$D/environment.txt"
cargo --version >> "$D/environment.txt"
uname -sm >> "$D/environment.txt"
printf '\nManifest/lockfile hashes (unchanged from admitted baseline):\n' >> "$D/environment.txt"
shasum -a 256 apps/rndr-home/Cargo.toml apps/rndr-home/Cargo.lock >> "$D/environment.txt"
cargo metadata --manifest-path apps/rndr-home/Cargo.toml --no-deps --format-version 1 > "$D/cargo-metadata.json"
find "$D/renders" -name '*.png' | sort | while IFS= read -r image; do shasum -a 256 "$image"; done > "$D/render-sha256.txt"
cat "$D/results.tsv"
# Docs checks run after the final human-readable inspection ledger is present.
