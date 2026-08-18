# H12 — Permanent quality gates

> **Status:** Implementing — hermetic lib tests, required Home `npm test`, workspace/Tauri Ubuntu gates, required docs, P01/P03/P09 micro-CI ratchets, and PR/nightly/release tiers. CI-001, TEST-001, and PERF-007 are **Mitigated on unit/CI** when those jobs are required. Validated/Shipped need retained nightly/release artifacts.
>
> **Accountable owner:** CI and release maintainers
>
> **Reviewers:** daemon, Home/Tauri, docs, performance owners (H03–H07, H09, H11)
>
> **Audit findings:** CI-001 (High), TEST-001 (High), PERF-007 (High)
>
> **Release gate:** Gate D — enforced architecture
>
> **Required decision:** none new (ADR-020 for package graphs; budgets live in verification records)
>
> **Dependencies:** All workstreams supply harness content; H12 owns the gates
>
> **Verification:** [Performance budgets](verification/performance-budgets.md), crash/concurrency and security matrices as companion evidence

## Outcome

Pull requests cannot merge while omitting the tests that currently exist.
`cargo test -p medousa --lib` is hermetic and repeatable under parallelism.
Home production tests and docs verification are required. Workspace crates and
the Tauri crate meet the same `-D warnings` bar on Ubuntu. Stable microbenchmarks
and dependency/bundle byte counts have checked-in ratchets. Noisy packaged
profiles stay on nightly/pinned hardware.

H12 does **not** wait for every other finding to reach Validated. It refuses to
merge without a harness/ratchet where one exists. Packaged soak, H06 multi-OS,
and FRONT-001 paint remain on their owners.

## Current evidence

WS5 shipped initial PR CI. This train made the remaining gates required:

- Home `npm test` (including generated guide/catalog freshness);
- workspace crate `--lib` tests and Ubuntu Tauri clippy/`--lib` tests;
- required `scripts/verify-docs.sh --strict`;
- `cargo deny` / machete / P09 dependency budgets;
- repeat-under-parallelism hermetic lib suite (`scripts/ci/test-hermetic.sh`);
- P01/P03 micro-CI plus nightly P04–P06.

TEST-001: `scoped_test_data_dir` is thread-local; eligibility unit tests inject
`CredentialAvailability`; `MEDOUSA_TEST_HERMETIC=1` skips the OS keyring and
panics if a unit test initializes the live ChatGPT OAuth broker.

PERF-007 harnesses exist as examples (`p01_turn_stream`, Home `bench:p02`,
`p04_forge_store`, `p05_coder_observation`, `p06_vault_backend`). P03 had no
harness. P08 bytes are already gated by H09.

## Invariants

1. Every release-blocking invariant is required somewhere before artifacts
   publish. Advisory `continue-on-error` is not a quality gate.
2. Unit tests do not consult the host keyring, real home directory, or network
   unless `#[ignore]` and labeled for an isolated integration job.
3. Process-global environment mutation in tests takes the suite env lock (or a
   thread-local like `TEST_DATA_DIR`).
4. The lib suite passes twice under default parallelism in CI.
5. Workspace crates excluded for cost/platform reasons are named. “Clippy
   compiled it” is not a test.
6. Micro-CI probes compare against checked-in JSON with median + noise
   allowance. A single lucky run cannot lower a ratchet.
7. Budget increases follow the performance-budget change protocol.
8. Nightly/release may run slower/noisier probes; they cannot be the only home
   of a deterministic correctness test.

## Non-goals

- full three-OS packaged Tauri on every PR;
- Criterion for its own sake when example harnesses already emit JSON;
- closing FRONT-001, H06 multi-OS Validated, or H04 P10 soak;
- making `medousa-sdk-iroh` clippy-clean in this train.

## CI tiers

### Pull request (required)

- `cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings`
- hermetic `cargo test -p medousa --lib` twice (`scripts/ci/test-hermetic.sh`)
- `cargo test --workspace --exclude medousa-sdk-iroh --lib`
- `cargo deny` + `cargo machete` + P09 dependency budget
- Home: `npm ci`, `npm run check`, `npm test`, `npm run test:h09`, `npm run build`, `npm run check:bundle-budget`
- `bash scripts/verify-docs.sh --strict`
- SDK contract, integrations, existing H02/H08 three-OS slices
- Tauri: Ubuntu `clippy -D warnings` (dead_code / too_many_arguments / large_enum_variant / ptr_arg / private_interfaces allowed until leftover proxy helpers are deleted) and `--lib` tests
- P01 micro fixture + P09 counts (P08 already in Home job)

### Nightly

- Tauri clippy/test on macOS and Windows
- P04/P05/P06 at their documented CI/modest sizes
- optional installer `cargo check`

### Release

[`release.yml`](../../.github/workflows/release.yml) remains the packaged ship.
The `quality-gates` job calls this repo's `ci.yml` so a release cannot publish
unless the PR-required set is green. H12 does not replace packaging.

## Hermetic test kernel

Production eligibility uses `target_ineligibility_reason`, which reads live
credential probes. Unit tests call
`target_ineligibility_with_credentials` with explicit booleans.

`crate::test_env` provides a process-wide mutex and RAII `set_var` guard for
tests that still must mutate the environment. Prefer thread-local data-dir
overrides. `scripts/ci/test-hermetic.sh` sets `MEDOUSA_TEST_HERMETIC=1`, which
skips the OS keyring and panics if a unit test initializes the live ChatGPT
OAuth broker. Each pass has a 20-minute wall-clock timeout on Ubuntu.

Genuine OS keyring/network/GUI tests are `#[ignore]` and run only in a named
integration job (none required on PR).

## Benchmark ownership

| Probe | Content owner | Gate owner | PR | Nightly |
| --- | --- | --- | --- | --- |
| P01 stream spine | H03 | H12 | small fixture | full matrix |
| P02 Home render | H03/H09 | H12 | — | pinned hardware |
| P03 feed/workspace | H04 | H12 | small fixture | larger N |
| P04 Forge store | H06 | H12 | — | CI-sized / modest |
| P05 Coder observation | H06 | H12 | — | CI-sized / modest |
| P06 vault | H07 | H12 | — | CI-sized / modest |
| P08 bundle bytes | H09 | H09/H12 | already required | packaged paint |
| P09 deps | H11 | H11/H12 | required | rebuild/size sample |
| P10 soak | H04 | H04 | — | packaged |

## Observability

Jobs retain logs and JSON probe output as CI artifacts. Synthetic fixtures
contain no user content. Machine class is recorded for nightly probes.

## Migration / rollout / rollback

Land gates only after the suite they require is green on the branch. Rollback
is reverting the workflow job and keeping the tests. Do not disable a red
required job with `continue-on-error`.

## Exit criteria

| Finding | Mitigated on unit/CI | Validated |
| --- | --- | --- |
| CI-001 | PR matrix above is required and green | nightly Tauri macos/windows plus one release workflow that includes the PR set |
| TEST-001 | hermetic lib suite twice under parallelism; no live keyring on unit path | same on a recorded runner image |
| PERF-007 | P01/P03/P08/P09 ratchets checked in and required | nightly P04–P06 artifacts retained; packaged P02/P08 paint still H03/H09 |

## Canonical documentation at ship time

- [`CONTRIBUTING.md`](../../CONTRIBUTING.md)
- this plan, hardening README ledger, performance-budgets PR/nightly policy
- packages/build cookbooks only when feature/build facts change (H11)

## Superseded code and concepts to delete

- docs job `continue-on-error: true`;
- “CI is green because we did not run the failing tests”;
- unit tests that open the host keyring to decide OAuth eligibility.

## Code anchors

- `.github/workflows/ci.yml`, `.github/workflows/nightly.yml`
- `scripts/ci/test-hermetic.sh`, `scripts/ci/check-perf-budgets.sh`, `scripts/ci/check-dependency-budget.sh`
- `src/inference_router.rs`, `src/test_env.rs`, `src/paths.rs`
- `examples/p03_feed_workspace.rs`, `crates/medousa-engine/examples/p01_turn_stream.rs`
- `scripts/verify-docs.sh`
