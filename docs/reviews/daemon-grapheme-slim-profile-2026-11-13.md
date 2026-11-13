# Native daemon Grapheme matches the existing embedded host profile

Date: 2026-11-13. Bounded continuation of the ownership/dependency undertaking.
Source/graph baseline: `4e85141b32d6c74a25427e7506758b38c8683739`.
Commit parent observed before sealing: `3b8c895ea6b36bcffd6ba0443f58fb67d7268592`.
The independent reviewer concurrently committed documentation-only changes in
`fa74520a` and `3b8c895e`; they are preserved as ancestry, not part of this
implementation's changes. No source overlapped. Its old-scope review is not
qualification of this new profile.

## Revised acceptance scope

The principal explicitly permits the daemon's Grapheme capabilities to narrow
to those of the existing slim daemon. Preserving all previous heavy modules is
no longer an acceptance gate. No upstream packaging fixes or blocker challenge
are pursued. The former [boundary report](daemon-grapheme-effective-boundary-2026-11-13.md)
is marked historical; its evidence remains version-bound and unchanged.
The independent review files present at entry are not edited or committed here.
Cancellation of that reviewer is not claimed. No integration, push, publication,
deployment, production changes, registry edits or broad cleanup are authorized.
Sender retains independent review and native integration responsibility.

## Actual reference, not an invented minimal profile

The root `Cargo.toml` **`embedded-daemon`** feature is the existing slim daemon
composition. `architecture/ios-embedded-daemon-plan.md`, under "Target capability
composition", calls its shared layer "lean Grapheme" and distinguishes full
native additions. `scripts/ci/check-ios-embedded-deps.sh` builds this actual
embedded daemon separately from the host-free SDK dependency probe.

Its Grapheme configuration at the parent revision is:

- Shared direct SDK dependency: `grapheme-sdk 0.7.1`, defaults disabled,
  `features = ["host"]`.
- `stasis-rs 0.13.0`, defaults disabled, with `stasis-rs/grapheme-host` selected.
- Direct optional compiler/runtime 0.7.1 dependencies, defaults disabled.
- No heavy SDK selectors and no in-process `grapheme-lsp` dependency.

Do not confuse this with `medousa-ios-dependency-probe/grapheme-portable`: that
isolated probe selects SDK **without host**. Nor is the SDK's empty `slim` alias
a daemon reference. Neither was chosen as the target.

Resolved Grapheme features, **identical for the unchanged embedded reference and
the revised native/Iroh daemon**, are:

| Package | Effective features |
| --- | --- |
| SDK 0.7.1 | `host` |
| Compiler 0.7.1 | none |
| Runtime 0.7.1 | none |
| Signatures 0.7.1 | none |
| Stdlib 0.7.1 | `email,host,http,net,sql,surreal,transforms,wasm,web` |
| WASM frontend 0.7.1 | none |
| Artifact 0.3.0 | none |

These are exact package/version and effective-feature comparisons, not merely a
count reduction. Stasis still selects the WASM frontend; the host bundle still
selects networking/storage/transform integrations. No zero-dependency claim is
made. The entire native daemon graph is naturally larger than embedded: native
Forge/Coder/Detamu, server and process-host services remain.

## Smallest coherent implementation

`full-daemon` remains the existing native-service composition and the binary's
required feature. Its Grapheme surface now matches `embedded-daemon`; the binary,
CLI, durable runtime, host bridge, secret grants, cancellation, admission,
observation drain and recovery implementations are not replaced or bypassed.
`Cargo.lock` is unchanged.

The previous heavy selectors and LSP dependency move into additive root feature
**`grapheme-extended`**, which includes `full-daemon`. `tui` selects it, preserving
its prior dependency/capability configuration. Full native deployments needing
the former surface can explicitly build:

```sh
cargo build --locked -p medousa --no-default-features \
  --features grapheme-extended --bin medousa_daemon
```

Default and ordinary `full-daemon`/Iroh builds intentionally use the slim surface.
No source launcher/release script is changed to silently re-enable heavy features.
Cargo feature unification still applies: building a TUI and daemon in one Cargo
invocation selects their union. Existing packaging tests enforce separate builds
and staging; slim daemon graphs below use standalone package/profile selection.

### Explicitly excluded from the slim native daemon

- Grapheme data/Polars, PDF, image, plot and media module implementations and
  their heavy compiler/signature feature branches.
- Stage-B AOT container/backend and WASIX runtime support. Ordinary compiler
  preflight/interpreter and Stage-A artifact APIs remain; this is not removal
  of every AOT-named API or of MIR host modules.
- The in-process Grapheme editor LSP, whose upstream compiler/signatures select
  `full`. Its WebSocket route retains the same declared authority/admission
  policy but returns **501 Not Implemented**, with an explicit build-feature
  explanation, and does not upgrade/start a server. The workspace metadata
  endpoint remains. Extended/TUI builds retain the original LSP path.

These exclusions can affect saved scripts using those capabilities. There is
no data migration or automatic substitution. Host capabilities, Medousa/shell/
scoped-secret dispatch, discovery and allowed-module policy remain. A configured
allowlist cannot enable an uncompiled module. Static discovery/signature records
alone are not claimed as proof of executable capability.

Medousa's separate image/document tools and direct dependencies such as `image`,
`calamine` and `pdf-extract` remain; not every similarly named package can be
removed merely because Grapheme's corresponding integration is excluded.

## Actual dependency differences

Standalone locked/offline host normal/build graph comparisons:

| Profile | Parent package-version pairs | Candidate | Delta |
| --- | ---: | ---: | ---: |
| Native daemon | 917 | 730 | -187 |
| Native daemon + Iroh | 1,002 | 830 | -172 |
| Embedded reference | 624 | 624 | 0 |
| TUI | 955 | 955 | 0 |
| Explicit extended daemon | 917 (old daemon) | 917 | 0 |

`graphs.py`/`graphs.json` in the evidence directory retain the full inventories,
removed package/version lists and every effective feature change. They assert
embedded's complete graph is unchanged, native/Iroh Grapheme package/features
match that baseline, TUI's non-root dependency/features are unchanged, and the
extended graph exactly restores the old daemon dependencies/features. No added
dependency packages are permitted in the slim native/Iroh comparisons.

The native daemon's actual removals include the Polars 0.46 family,
`comfy-table 7.2.2`/`crossterm 0.29.0`, `grapheme-lsp 0.7.1`/`tower-lsp`,
`grapheme-aot-container 0.7.1`, and the Wasmer/WASIX/Cranelift backend families.
There are 187 removed package/version pairs, zero added, and 20 common-package
feature-set changes; the complete lists are
in `graphs.json`. Iroh independently retains some packages, hence its smaller
172-package delta. None of this drops Medousa's native service dependencies.

This is not a release binary-size, compilation-time, startup, RSS or latency
measurement. Different profiles can retain some removed branch dependencies
through unrelated consumers (notably Iroh); counts are not module inventories.
The CI dependency guard now rejects **all** Crossterm/Ratatui dependencies in
slim daemon profiles, replacing the former comfy-table exception, and compares
the actual embedded Grapheme closure. Extended/TUI heavy-feature and terminal
positive controls remain.

## Fresh qualification

All checks below are **fresh on the final formatted implementation**, Rust
1.100.0/Cargo 1.100.0 on aarch64 macOS, locked/offline, existing cache, serial
jobs=1. No historical binary or old 47/51-test result is reused as qualification.

- Native `full-daemon` binary type-check: **PASS**.
- TUI and source launcher type-check under `tui`: **PASS**. This also checks
  the preserved extended LSP implementation; no live LSP roundtrip is claimed.
- Fresh `full-daemon` lib test executable compiled/linked: **PASS**, followed by
  **59 unique focused Rust tests, all passing**:
  - 3 actual platform constructor/lifetime selectors in separate processes;
  - 1 canonical TempDir/symlink ownership-cleanup regression;
  - 47 original event-drain/runtime identity/settings/tool contract/spine parity/
    catalog/durable Grapheme/recovery/HTTP admission tests;
  - 2 new slim-profile tests: explicit LSP 501 and core execution/heavy-import
    rejection;
  - 6 retained-host tests: shell workflow, dropped-worker cancellation, scoped
    secret engine/workflow dispatch, and direct/engine unapproved HTTP denial.
- Packaging/harness Python regressions: **9/9 PASS**, including separate-build
  staging/overwrite defenses and isolated-runner controls.
- Resolved dependency guard (default, explicit daemon, Iroh and extended/TUI
  positive controls), full before/after comparison: **PASS**.
- Runtime ownership Python equivalent, targeted Rust format, Python AST,
  Bash syntax, strict docs, source/evidence auditor and whitespace: **PASS**.
  `rg` remains unavailable; its shell guard is not counted green.

The final test executable SHA-256 is
`3c000dcea20ec36d682d7c06288f881fa376a71187e6aecad5b054a3a55d4693`.
`binary.json` includes Cargo's exact fresh artifact and feature selection. Test
selectors and expected counts are in `qualify.py`; zero matches cannot pass.

The initial attempt passed 53 Rust tests and packaging/graph checks, then stopped
at two rustfmt-only assertion layouts. That failure, original binary hash,
command receipts and raw logs are preserved in `initial-qualification.tar.gz`
and `initial-*` files. The layouts were corrected, the exact final source was
freshly checked/recompiled and **all 59** final selectors executed successfully.
No substantive compiler/test failure is hidden as a format failure.

Minimum sampled free space across the attempts was **9.770210266 GiB** (at final
lib linking), above the **8 GiB** stop and **6 GiB** hard floor. No disk stop,
cache/worktree removal, profile-wide rebuild command or upstream edits occurred.
Final report/provenance checks use no further compilation.

## Evidence and reproduction

Evidence: `docs/reviews/daemon-ownership-evidence/slim-grapheme-2026-11-13/`.

- `before-*.txt.gz` are graphs captured before the manifest edits, at the parent.
- `after-*.txt.gz`, `graphs.py`, `graphs.json` bind actual candidate comparisons.
- `qualify.py` uses the existing disk monitor, serial jobs=1, locked/offline Cargo
  and the configured cache, normal host TMPDIR, no cache/worktree removal. It
  samples at two-second intervals and interrupts at **8 GiB**, above the **6 GiB
  hard floor**. The isolated constructor runner owns its 32 MiB test stack.
- Logs and command/exit/test-count/disk receipts are retained, including failures;
  no zero-test match is counted as passing.

No full workspace/Clippy, release link/size, runtime performance, cross-platform,
native media/AOT/WASIX roundtrip, Docker/PowerShell or production qualification
is implied. Iroh is graph-qualified here, not freshly compiled. Upstream sources
and supported full/TUI capability implementations are not modified.
