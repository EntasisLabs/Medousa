# Medousa inference MIR-0–2 release checklist

Status date: 2026-08-02. Branch: `medousa-brain`.

This scorecard separates implemented safety from physical evidence. A backend
or machine is not release-qualified merely because its collector compiles.

## Engineering status

| Milestone | Implemented release behavior | Remaining evidence |
|---|---|---|
| MIR-0 | Content-free exact-identity manifests; bounded context/batch overrides; dry-run-first matrix and soak runner; fail-closed ranked gate report | Run representative UQFF and ISQ artifacts on qualified machines |
| MIR-1 | Truly cold startup/onboarding; request-time single-flight activation; verified generation handshake; load cancellation; bounded unload/kill; idle, provider/model switch, app-exit, macOS/Windows/Linux sleep eviction | Complete 100-cycle and crash/restart lifecycle evidence |
| MIR-2 | Host/device admission; cross-process leases; calibrated peaks; Metal, NVML, AMD SMI, WDDM, and Vulkan budget normalization; live critical host/device pressure eviction; fixed 4K/context, batch/concurrency 1 | Execute the Metal/CUDA/ROCm/WDDM hardware matrix and promote only passing recipes |

## Automated release gates

The repository gate is:

```bash
cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings
cargo test -p medousa --lib
(cd apps/medousa-home && npm run check)
scripts/verify-docs.sh
```

Current branch result: Clippy passes, all 693 daemon library tests pass, Home
reports 0 errors and 0 warnings, and documentation verification passes. The
local inference governor has 51 passing tests, the local engine has 2, shared
types have 25, benchmark parsing has 5, and Home lifecycle additions have 6.

## Physical evidence gate

For every promoted recipe/device identity:

1. Install the exact pinned model package; benchmarking never downloads.
2. Preview the matrix without `--execute` and verify its run count.
3. Run 100 cold load/request/unload cycles per context/batch cell.
4. Analyze the directory with `analyze-local-inference-benchmarks.py`.
5. Do not promote unless identity and completion are exact, peak prediction is
   within 15%, swap does not grow, at least 95% of the measured footprint is
   reclaimed at 10 seconds, and settled RSS grows no faster than 1 MiB/cycle.
6. Kill the worker during load and during decode, restart Home, switch provider,
   and suspend/resume once; the daemon and app must survive and remain cold.

## Current-machine finding

The local worker was confirmed cold: no listener exists on `127.0.0.1:7421`.
The fresh development worker advertises `cpu` and `metal`. No model package is
installed. The live host probe reported a 16 GiB Apple machine with only 1,945
MiB available during validation, below the 4 GiB system reserve. Running a model
matrix in that state would correctly be refused and would not be valid release
evidence.

The next safe local evidence run is the 1.1 GB UQFF
`gemma-4-e2b-it-qat`, after restoring sufficient memory headroom. CUDA, ROCm,
and Windows WDDM qualification require their physical target machines; those
results must not be inferred from this Apple host.
