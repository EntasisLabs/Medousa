#!/bin/sh
# Additive offline reproduction; defaults never overwrite committed evidence.
set -eu
export CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
# Bound CPU contention as well as Cargo jobs. Full-width parallel tests have
# reproduced loopback fixture timeouts, not a changed endpoint assertion.
export RUST_TEST_THREADS=2
export OUT=${OUT:-.desktop-correction-repro}
export TARGET=${TARGET:-.desktop-correction-repro-target}
if [ -e "$OUT" ] || [ -e "$TARGET" ]; then
    echo 'Refusing existing output/target; choose fresh scratch paths.' >&2
    exit 1
fi
sh apps/rndr-home/docs/desktop-evidence/reproduce.sh
# The shared script records docs failure without failing the shell; assert it.
awk -F '\t' '$2 != 0 { bad=1 } END { exit bad }' "$OUT/results.tsv"
# Remove only this run's own cache after retaining the evidence and disk receipt.
rm -rf "$TARGET"
