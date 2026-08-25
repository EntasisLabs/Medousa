# H11 — Package and dependency boundaries

> **Status:** Implementing — inventory, unused-root-dep removal, cargo deny/machete, package composition map, and P09 CI ratchet are in this train. DEP-001 is **Mitigated on unit/CI** once those gates are required. Validated still needs a named clean-build/link/size sample on a recorded machine.
>
> **Accountable owner:** daemon composition and packaging maintainers
>
> **Reviewers:** Home/Tauri, install-support, adapters, release engineering, security
>
> **Audit finding:** DEP-001 (High)
>
> **Release gate:** Gate D — enforced architecture
>
> **Required decision:** [ADR-020](../../docs/architecture/decisions/adr-020-feature-boundaries-and-lazy-runtime.md)
>
> **Dependencies:** Home-first package catalog; H09 capability descriptors; H10 feature-profile routes
>
> **Verification:** [Performance budget P09](verification/performance-budgets.md)

## Outcome

The default engine compile/link graph is an owned product surface. Optional
workloads (channel adapters, MCP gateway binary, local-brain worker, Iroh,
OpenTelemetry) are pay-for-play packages or explicit Cargo features. Unused
direct root dependencies are gone. CI fails on unique-package or
duplicate-version growth unless the budget file changes with justification.

H11 owns Cargo/dependency/binary budgets and the package-id ↔ crate/feature/binary
map. ADR-020 owns the shared capability rule. H09 owns Home UI chunk loading.
H12 owns making these checks required CI.

## Current evidence

2026-08-17 `cargo tree -p medousa -e normal --no-dedupe` (unique name/version):

| Metric | Audit 2026-08-12 | Frozen H11 inventory |
| --- | ---: | ---: |
| Unique normal name/version pairs | 932 | 944 pre-cut / **900** after unused root deps and the runtime boundary |
| Duplicate-version crate names | 93 | 94 pre-cut / **90** after unused root deps |

The audit ceilings are historical. The binding ratchet is the checked-in
[`scripts/ci/dependency-budget.json`](../../scripts/ci/dependency-budget.json)
file; later changes lower it or carry a reviewed budget-change record. Profiles recorded separately: default
engine, `iroh-transport`, adapter workspace (`telegram`/`discord`/`slack`), and
Home Tauri. Duplicate-version *names* are ratcheted; notable stacks
(`reqwest`/`rustls`/`tungstenite`/`tonic`/`genai`/`schemars`/`sysinfo`) have
owner + revisit dates in `duplicateVersionLedger`.

### 2026-08-23 `medousa-runtime` boundary

- **Metric/workload:** P09 package closure for `medousa-default` and
  `medousa-iroh-transport`.
- **Raw comparison:** default unique name/version pairs `899 -> 900`; Iroh
  `980 -> 981`; Home Tauri `490 -> 491`; duplicate-version names remain `90`,
  `96`, and `33`. The added package is the local `medousa-runtime` crate.
  Its dedicated normal dependency closure is ratcheted at `537` unique
  name/version pairs and `43` duplicate-version names; this is the current broad
  Stasis native graph, not the desired mobile minimum. Phase 1B added `chrono`, `serde`,
  `serde_json`, and `medousa-engine` edges to that crate, but every package was
  already in both measured closures. Phase 1C added direct lean Stasis, `genai`,
  `tracing`, `sha2`, Tokio, and `tokio-util` edges for the production
  transcript/checkpoint, perception-policy, and cancellation/deadline seams;
  the portable loop-gate and foreground-presentation-port seam adds no
  production dependency. Physically moving the production loop and golden suite
  adds only a test-only `async-trait` edge, whose package was already present in
  the measured workspace closures. The ratchets therefore remain `900` and
  `981`.
- **Product/users/hardware:** the boundary lets desktop/server and iOS
  deployments of the daemon share one foreground-turn kernel. Default and Iroh builds see one
  additional workspace package node; no external library, runtime service, or
  binary service was added. The initial policy kernel checked for
  `aarch64-apple-ios` in 0.32 seconds; after the portable state/engine edges,
  target-local checks completed in 10.69 seconds for device and 6.87 seconds for
  simulator. With the direct lean Stasis/`genai` seam, clean dedicated-cache
  checks completed in 68 seconds for device and 54 seconds for simulator; this
  measures the current broad Stasis native graph and is not a release-size
  claim.
- **Correctness/security/durability:** production completion policy moved rather
  than forked, existing in-tree imports use compatibility re-exports, the root
  library check and golden suite pass, and concrete credential, daemon
  filesystem, network, delivery, and worker authority remain outside the
  portable loop through explicit ports. The daemon's typed execution identity
  is carried only as an opaque context token while the portable boundary owns
  cancellation and absolute-deadline enforcement. The completion gate now
  carries only portable state and ports; daemon stream presentation is an
  adapter and remains optional for adapter-minimal compositions. The production tool loop
  now has one implementation in `medousa-runtime`, its 12 golden turns execute
  with only the adapters they exercise, and the root module is a compatibility re-export.
  Daemon/TUI consumers import that implementation directly while assembly
  retains live product and environment settings through an injected
  parallel-execution-settings provider.
- **Alternatives:** leaving the FSM coupled to host-only daemon modules prevents
  target-specific daemon composition; copying it creates a second completion authority; placing
  foreground policy in the event-spine crate conflates orchestration with
  persistence.
- **Owner/revisit:** iOS embedded-daemon and daemon composition maintainers; revisit
  when the mobile daemon composition lands or 2026-11-01, whichever comes
  first.
- **Workstream/finding:** the portable-runtime milestone is tracked in the iOS
  embedded-daemon plan. DEP-001 remains mitigated because P09 still
  ratchets the updated counts, duplicate counts did not grow, and the extraction
  introduced no new package. Stasis feature pruning remains explicit mobile
  size debt rather than a second Medousa runtime implementation.

Root [`Cargo.toml`](../../Cargo.toml) listed `teloxide`, `serenity`, and
`slack-morphism` with no uses under `src/`. Adapter crates already own those
frameworks and ship as Settings → Packages binaries.

Default Cargo features remain `[]`. Optional compile-time features are
`iroh-transport` and `otel-export`. Grapheme, vault, calendar, browser-lite, and
document extract stay in the Home-first engine.

## Invariants

1. The default `medousa` / `medousa_daemon` link does not compile channel-adapter
   frameworks (`teloxide`, `serenity`, `slack-morphism`).
2. Settings → Packages binaries are separate workspace crates or optional
   features; advertising availability never pulls their implementation into the
   default daemon.
3. Thin clients to optional workers (MCP gateway HTTP, local-inference handshake)
   may stay in the engine; the worker binaries do not.
4. A new dependency has an owner, feature/package justification, P09 delta, and
   duplicate-version explanation.
5. Duplicate-version names are an explicit ledger (`deny.toml` skips). A new
   duplicate name is a CI failure.
6. Unique name/version count and duplicate-name count cannot rise without a
   budget-file change that follows the performance-budget protocol.
7. Vault, Grapheme, calendar, browser-lite, and media extract remain in the
   default engine. H11 does not feature-gate them out of Home-first.

## Non-goals

- making a tiny daemon that cannot chat, vault, or run Grapheme;
- claiming 944 packages equals 944 packages in the final binary byte-for-byte;
- unifying every transitive HTTP/TLS stack in one car;
- closing FRONT-001 or packaged-app size (H09 / release);
- making `medousa-sdk-iroh` clippy-clean.

## Canonical ownership

| Package id | Workspace crates | Cargo features | Binaries | Default engine link |
| --- | --- | --- | --- | --- |
| `engine` | `medousa`, `medousa-runtime` | default `[]` | `medousa`, `medousa_daemon`, `medousa_cli`, `medousa_tui` | yes |
| `desktop` | `medousa-home` (Tauri, excluded workspace) | n/a | Medousa.app | no |
| `local-brain` | `medousa-local-inference` | n/a | `medousa_local` | thin client only |
| `mcp-gateway` | `medousa-mcp-gateway` | n/a | `medousa_mcp_gateway` | thin client only |
| `adapter-telegram` | `medousa-telegram` | n/a | `medousa_telegram` | no |
| `adapter-discord` | `medousa-discord` | n/a | `medousa_discord` | no |
| `adapter-slack` | `medousa-slack` | n/a | `medousa_slack` | no |
| `adapter-whatsapp` | (engine-launched helper) | n/a | `medousa_whatsapp` | no |
| `coding-engine` | `medousa-code` | n/a | `medousa-code` | no |
| `shell-session` | `medousa-session` | n/a | `medousa-session` | no |
| `iroh-transport` | `medousa-iroh-http` | `iroh-transport` | (engine feature) | off by default |
| `otel-export` | n/a | `otel-export` | (engine feature) | off by default |

`stasis-rs` `grapheme-full` plus direct `grapheme-*` crates both remain: Stasis
embeds a Grapheme host; Medousa owns workshop/LSP/runtime. Do not drop either
without a dedicated compile probe.

## Threat / failure model

- Silent compile of unused adapter stacks enlarges advisory/update surface.
- Duplicate TLS/HTTP generations hide CVEs behind “we already patched reqwest.”
- Umbrella features (`grapheme-full` plus every integration) make Home-first
  install compile optional product.
- Budget files updated upward “to make CI green” erase the finding.

## Concurrency, cancellation, durability, limits

Not applicable beyond build/link resource use. P09 records check/build/link time
and peak memory as diagnostic metrics until a named machine accepts a target.

## Observability

`scripts/ci/check-dependency-budget.sh` prints unique pairs, duplicate names,
and banned-root-dep hits. `cargo deny` prints advisory/license/ban/source
failures. Do not log registry tokens or private git URLs.

## Migration / rollout / rollback

1. Inventory + budget file (no behavior change).
2. Delete unused root adapter frameworks; keep adapter packages.
3. `cargo deny` + machete with reviewed skips.
4. Encode the package map in install-support and JSON.
5. Require P09 in CI.

Rollback restores the previous `Cargo.toml` / `Cargo.lock` / budget file
together. Do not keep a lower budget with the old lockfile.

## Tests, benchmarks, exit criteria

DEP-001 is **Mitigated on unit/CI** when:

- root `Cargo.toml` has no `teloxide` / `serenity` / `slack-morphism`;
- `check-dependency-budget.sh` is required CI and matches the frozen file;
- `cargo deny` and `cargo machete` are required with reviewed exceptions;
- `package_composition()` matches every catalog id;
- default engine compile does not pull adapter implementation crates.

**Validated** additionally requires a recorded clean-build/link/size sample
(machine id, feature set, artifact bytes) in the performance-budgets record.

## Canonical documentation at ship time

- [`docs/guides/packages.md`](../../docs/guides/packages.md)
- [`docs/cookbook/build-from-source.md`](../../docs/cookbook/build-from-source.md)
- this plan, hardening README ledger, P09 section

## Superseded code and concepts to delete

- unused root adapter-framework dependencies;
- the idea that Clippy compiling the workspace is a dependency policy;
- undocumented duplicate-version stacks without owners.

## Code anchors

- `Cargo.toml`, `deny.toml`, `scripts/ci/dependency-budget.json`
- `crates/medousa-install-support/src/packages.rs`
- `src/bin/medousa.rs` package launch paths
- `adapters/medousa-{telegram,discord,slack,mcp-gateway}`
