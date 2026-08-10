# Semantic typing performance evidence

This record covers the performance gate for the semantic-typing and
construction epic. It uses the reproducible warm benchmark:

```bash
scripts/benchmark-semantic-typing.sh
```

The benchmark runs incremental `cargo check` plus focused tests for
compatibility schemas, recurring construction/binding, runtime composition,
job construction, and assembled first-party contracts. It is a regression
probe for compile and process-level execution cost, not a statistically tuned
microbenchmark.

## Measurement: 2026-08-09

Environment: macOS workspace, repository target cache at
`../.cache/cargo-target`, current semantic-typing tree at commit
`62fbc3cc` plus only the benchmark files being added with this record.

Steady-state warm run:

| Probe | Real time |
|---|---:|
| Incremental library check | 0.51 s |
| Compatibility schemas and wire behavior | 0.41 s |
| Recurring construction and binding | 0.42 s |
| Runtime composition dispatch | 0.41 s |
| Job specification construction | 0.41 s |
| Assembled first-party contract | 0.64 s |

All focused tests passed. The first run after the schema-compatibility slice
recompiled the library in 6.30 s; the immediately repeated check settled at
0.48 s, so the one-time rebuild is not treated as a hot-path regression.

## S0 comparison

An isolated archive of S0 parent `f8005883` was checked with the same Cargo
workspace configuration and a separate target cache:

| Probe | S0 observation |
|---|---:|
| Clean library check | 121.48 s |
| Warm library check | 0.50 s |
| Representative recurring-feed test, steady state | 1.53 s |

The S0 clean check includes the full dependency build and is not directly
comparable to the current 6.30 s semantic-crate rebuild. The useful like-for-
like signal is the warm library check: current 0.48 s versus S0 0.50 s, within
normal process noise. The recurring focused probes remain sub-second in the
current tree. No unexplained compile or focused execution regression was
observed, and the migrated recurring paths contain no internal typed-to-JSON
round trip.

These measurements do not justify a claim about all production workloads;
future hot-path changes should rerun the script and add a narrower benchmark
if a real regression appears.
