# Grapheme effective compiled-surface boundary: blocked in this repository

Date: 2026-11-13. Evidence-only continuation of the same pre-migration
ownership/dependency undertaking. **No effective slimming implemented or claimed.**

## Decision and exact scope

Baseline: `6e64e1d6ac359ea223baf6754d219ca2b2148a82`, clean at entry. This includes
the accepted independent harness-review deliverable over `9026f46`; it does not
mean that the whole undertaking was accepted or integrated.

With the current published Grapheme 0.7.1 / Stasis 0.13.0 packaging, I did not
find a safe effective dependency/feature reduction inside Medousa that preserves
the required daemon and TUI functionality. The narrow concrete candidate is
**upstream Grapheme stdlib's Polars formatting feature**, which Medousa cannot
subtract. Return this blocker rather than changing selectors without changing
the resolved implementation surface, or silently removing supported capabilities.

Only this report and a new evidence directory are added. All baseline tracked
files, including Cargo manifests/lockfile, source, release/CI guards, both P2
fixes, historical reports, archives and verifiers, retain their working bytes.
There is no new functional candidate to qualify. No upstream source, registry
cache, other worktree, production state or format was edited. No migration,
Forge restructure, push, publication, deployment, download or spend occurred.
Sender retains independent review and native integration responsibility.

## Inputs and method

Read both original sibling architecture/performance audits in the separate
pre-migration audit worktree, particularly architecture R6 and the performance
Grapheme section. Their hashes and Grapheme excerpts are in
`sibling-audit-inputs.json`; neither sibling file was modified. Also read the
original ownership/dependency report and the harness/independent evidence and
verifiers. Their earlier qualifications remain version-bound, not relabeled.

Inspected actual registry manifests for SDK, stdlib, signatures, compiler,
runtime, AOT container, WASM frontend, LSP, Stasis, Polars/core and comfy-table.
Exact normalized manifests, hashes, dependency/target/feature declarations,
upstream Rust source hashes and line-addressable cfg/dispatch excerpts are
saved. All 341 inspected upstream manifest/source files across 12 packages were
also compared with cached `.crate` archives whose SHA-256 matches `Cargo.lock`;
`upstream-registry-checksums.json` records that additional source binding. Used **standalone package normal/build graphs**, not workspace/dev-unified
metadata, as the effective feature authority. `workspace-declarations.json.gz`
is supplementary `cargo metadata --no-deps`, not a claim about resolved features.

Before and after commands (one invocation per profile):

```sh
cargo tree --locked --offline -p medousa --no-default-features \
  --features full-daemon -e normal,build --prefix none \
  --format '{p} FEATURES=[{f}]'
# Repeat for tui and full-daemon,iroh-transport, separately.
```

The snapshots bracket inspection/qualification, **not an applied optimization**.
The JSON inventories include every package-version and resolved feature; raw
trees are losslessly gzip-compressed. Five feature-inverse graphs identify who
forces stdlib/compiler/WASM/Crossterm/WASIX into the host graph. The commands and
fresh execution receipts are saved. Raw command logs and the line-numbered
upstream output excerpt are stored losslessly as gzip files in the checkout;
the archive retains their original uncompressed bytes and names.

## Effective before/after evidence

| Host profile | Before package-version pairs | After | Effective feature/source reduction |
| --- | ---: | ---: | --- |
| Standalone daemon | 917 | 917 | None |
| TUI / terminal launcher | 955 | 955 | None |
| Standalone daemon + Iroh | 1,002 | 1,002 | None |

All three raw resolved graph hashes match exactly before/after; **no package,
resolved feature or application source is removed or added**. These are not
compilation-unit counts, linked-size, RSS, startup or latency measurements.

The identical cfg-enabled Grapheme source surface is established by exact
upstream source hashes, resolved features and the source gates in
`compiled-source-gates.txt` / `upstream-dispatch-and-output.txt`, not by package
counts alone:

- SDK: `data,host,image,media,pdf,plot,stage-b,wasix-runtime`.
- Stdlib: `data,email,host,http,image,media,net,pdf,plot,sql,surreal,transforms,wasm,web`.
  `core/json` are always present; `wasm` also selects transforms.
- Runtime: `data,image,media,pdf,plot,stage-b,wasix-runtime`.
- Compiler: `data,full,image,media,pdf,plot,stage-b`.
- Signatures: `data,full,image,media,pdf,plot`; LSP: `default,full`.
- AOT container, WASM frontend and WASIX backend dependencies remain present.

`inspect.py` asserts the required implementation **and** compiler/runtime/signature
closure for every profile. Stdlib `src/lib.rs` gates implementations by feature;
`src/registry.rs` gates host dispatch; SDK `src/lib.rs:1906` actually dispatches
through that registry. Runtime `src/runtime.rs` has explicit disabled-feature
errors for Stage B/WASIX, not equivalent alternative execution. Source-gate
inspection is not a new native media/AOT/WASIX roundtrip test or a rustc unit graph.

## Concrete forcing edges and retained callers

### 1. Data -> TTY: the smallest actionable blocker

The saved normalized `grapheme-stdlib-0.7.1-Cargo.toml` declares:

```toml
[dependencies.polars]
version = "0.46"
features = ["csv", "fmt"]
optional = true
default-features = false
```

The exact effective chain is:

```text
Medousa full-daemon -> SDK/data -> stdlib/data -> Polars 0.46/csv,fmt
Polars/fmt -> polars-core/fmt -> comfy-table/tty
comfy-table 7.2.2/tty -> crossterm 0.29.0
```

Polars already exposes `fmt_no_tty -> polars-core/fmt_no_tty -> comfy-table`
without `comfy-table/tty`. But features are additive: requesting `fmt_no_tty`
from Medousa **cannot turn off** stdlib's `fmt`. `default-features=false` does
not override an upstream explicit feature. Dropping `data` would remove supported
CSV/data-frame implementation, not merely terminal decoration.

`grapheme-stdlib/src/data/frame.rs:118-172` serializes data-frame columns/rows to
JSON and formats dtype/fallback cell values. That is why even an upstream switch
needs output/error parity checks, not a claim that formatting is irrelevant.
The inspected function does not render the whole DataFrame as a terminal table.

### 2. Host selection is not an execution allowlist

Root SDK explicitly selects `host`. Independently,
`Stasis/dashboard -> grapheme-host -> grapheme -> SDK` and
`Stasis/grapheme-host -> SDK/host` force it. `SDK/host -> stdlib/host` selects
`transforms,http,web,net,email,sql,surreal`. Removing just the root `host` request
would therefore not slim the implementation. Removing the bundle or dashboard
would remove supported functionality/routes, not preserve them.

`src/grapheme_workshop.rs:33-49` treats an empty configured allowlist as unrestricted,
and otherwise enforces the configured set. A default/current allowlist is not a
product-level declaration that all other modules may be compiled away. Discovery
uses static manifests (`src/grapheme_host_catalog.rs:112`); discovery alone is
not proof of execution availability. The SDK's default host still supplies stdlib
dispatch alongside Medousa's interceptor.

### 3. Runtime, authoring and signatures have real obligations

- `src/grapheme_workshop.rs:120-180` implements check/AOT compilation. Its direct
  `compile_source_to_aot` call is Stage A; it is **not** evidence that this call
  directly invokes the Stage B compiler. Separately, SDK `wasix-runtime` implies
  `stage-b`, so removing only the explicit Stage B selector cannot subtract it.
- `src/grapheme_workshop.rs:214-263` activates module generations with WASIX/MIR
  ABI and compatibility policy. `src/grapheme_medousa_bridge.rs:81-89` installs
  the hotload store plus host registry and the capability interceptor. Removing
  WASIX breaks supported module execution; upstream runtime has an explicit
  built-without-WASIX error. No interpreter-only substitution was introduced.
- `src/grapheme_lsp_bridge.rs:46` starts the in-process editor server. LSP's
  compiler and signatures dependencies **unconditionally request `full`**, even
  if its own default feature is disabled. Thus changing its root
  `default-features` flag alone does not reduce signatures/compiler surface.
- `src/grapheme_runtime.rs` retains compiler preflight and durable job/attempt
  matching; `src/portable_grapheme_engine.rs` and the Medousa bridge retain source,
  import, step/depth, secret, cancellation and effect-time authority controls.
  These were not bypassed to make a smaller build.

Stasis also selects `grapheme-wasm` through `grapheme`, although its native-host
WASM-workflow-adapter consumer is cfg-disabled (`infrastructure/runtime/mod.rs:11-15`).
That is another **upstream feature boundary**: the optional dependency is tied to
an aggregate feature in Stasis, and Cargo builds the selected dependency even
when that consumer module is cfg-disabled. Dropping Stasis's whole `grapheme`
feature would also drop the retained native SDK adapter. The frontend shares
compiler/runtime/stdlib with the host; its package count is not evidence of a
second resident runtime or large incremental cost.

## Minimal follow-up design — proposed only, not authorized or implemented here

**First choice: a narrow upstream Grapheme stdlib patch release.**

1. In Grapheme's own authorized work, evaluate replacing the explicit Polars
   `fmt` request with `fmt_no_tty`, retaining `csv`, `data`, all operations,
   signatures and compiler/runtime wiring. Do not remove formatting wholesale
   or change returned shapes. If parity cannot be established, stop rather than
   accepting changed data formats. No need to split Medousa's engines or stores.
2. Compare CSV/data-frame outputs and errors for empty/null/bool/numeric/string
   data, dtype strings, fallback scalar values and malformed inputs. Include
   roundtrips and catalog/signature/implementation equality. Check no other
   dependency re-enables Polars `fmt`/comfy-table `tty`. Qualify the upstream
   supported target matrix. Preserve AOT/WASIX and policy behavior.
3. After a reviewed published patch exists and downstream work is separately
   authorized, update only the necessary locked package(s) in Medousa. No local
   registry edits, `[patch]` vendor fork, runtime download or format migration.
   Re-capture exact standalone feature/package/source inventories and verify the
   TTY branch really disappears from the daemon. Update the dependency guard's
   *current requirement for the retained comfy-table/Crossterm edge* to assert
   absence, keeping TUI positive controls. That guard change is intentionally
   **not** made now; current dependencies still need its exception.
4. Re-run daemon/TUI checks and the focused security/durability/recovery tests,
   plus data execution parity. If space permits, record source-bound compiled
   units/linked artifacts before/after. Do not promise release size or speed
   improvements from a graph delta alone. Roll back via the lockfile/manifest
   change, without touching persisted data.

The expected narrow win is removal of the **daemon's** comfy-table TTY/Crossterm
branch, not removal of Polars or the TUI's terminal support. It is a proposal,
not an untested claim that a switch has already preserved every behavior.

**Optional separate upstream Stasis follow-up:** separate a shared Grapheme core
feature from the WASM adapter dependency, route native `grapheme-host` to the
core, and leave a backward-compatible aggregate selection for consumers needing
both. Test native SDK and browser/non-host adapter matrices. This can remove an
unneeded native frontend compilation but does not by itself remove the shared
compiler/runtime/stdlib. It is not needed for the first, smaller formatting fix.
Do not bundle broad SDK/LSP/host decomposition or new optional product profiles
into this undertaking to manufacture a larger saving.

## Fresh qualification and version binding

Host: Rust/Cargo 1.100, aarch64 macOS. Offline/locked, jobs=2, serialized commands.
Active monitor samples every two seconds, interrupts at **8 GiB**, preserving
the **6 GiB** hard floor; no cache/source/worktree deletion. Minimum across the
recorded qualification/inspection/docs commands: **16.3607 GiB**. No stop occurred.

- Standalone daemon check and TUI + terminal-launcher check: **PASS**, cached
  Cargo validation, not a clean rebuild. Existing redundant-homepage and
  `proc-macro-error2` future-compat warnings remain visible.
- Cargo `test --lib --no-run --message-format=json`: **PASS**, authenticated
  artifact `fresh=true`; executable SHA-256
  `c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03`.
  Same corrected source/binary as the independent review, not rebuilt here.
- **51 unique Rust tests freshly executed and passed**: 3 actual constructor
  tests through the unchanged committed isolated runner, 1 symlink fixture test,
  and the original 47 focused tests. Exact nonzero selectors/commands/counts are
  in `qualify.py` and `receipts.json`; each required summary is asserted. Normal
  macOS TMPDIR inherited; no caller stack override. The committed runner owns
  its fixed stack. Test groups cover drain/identity/settings, contracts and turn
  bytes, Grapheme catalog/admission/cancellation/receipts, recovery and HTTP
  authority boundaries. This is cached-binary execution, not fresh compilation.
- Packaging/harness Python checks: **9/9 PASS**.
- Existing dependency guard: **PASS**, default/explicit/Iroh daemon and terminal
  controls, implementation/signature closure retained. Fresh before/after
  capability assertions: **PASS** for all three profiles.
- Python equivalent ownership scan: **PASS**, 781 Rust files. `rg` is unavailable;
  the original shell guard is **not green**, not repaired or counted as passing.
- Historical harness and independent evidence auditors: **PASS** against their
  original version-bound records; they are audits, not extra Rust executions.
- Final new evidence auditor, Python AST, strict docs and diff-whitespace checks
  are recorded with the sealed handback. Initial new collector diagnostics
  rejected CRLF assumptions; only this new collector was corrected to use Git
  archive's committed checkout-attribute semantics. They are not product failures
  or passing qualification gates. A checksum-metadata discovery probe also
  failed because this cache lacks `.cargo-checksum.json`; that probe is not
  counted green. The explicit locked `.crate` archive comparison above passed
  instead. The first staged whitespace check also rejected libtest's raw extra
  EOF blank lines and blank numbered excerpt lines. Those raw evidence bytes
  were retained losslessly in gzip form rather than edited; the final staged
  check is separate from the earlier baseline-only diff check. Historical
  verifiers were not altered.

`baseline-files.json` binds **7,742** tracked baseline working files to the
baseline Git archive with its declared checkout attributes (including CRLF).
All those working bytes are checked again at handback. Thus the new checks/tests
qualify the exact unchanged application source; new scripts/reports have their
own file hashes and archive. Historical fingerprints were neither overwritten
nor extended to pretend to qualify a different source revision.

**Not run/claimed:** full rebuild, Docker build, release linking/size or rustc
unit-graph comparison; full workspace tests/Clippy, TUI unit suite, Windows/Linux/
wasm target checks, native GUI/media/AOT/WASIX roundtrips, Iroh compilation, remote
store/restart matrix, live daemon/production tests, runtime performance or
platform certification. The heavy-capability retention proof here is unchanged
source + resolved implementation/signature closure + authenticated unchanged
binary, not new end-to-end coverage of every heavy module.

## Handback and next decision

This is a concrete upstream-packaging blocker, **not completion of effective
Grapheme slimming or the whole undertaking**. Review the evidence and authorize
the narrow upstream stdlib formatting investigation separately if desired.
Medousa functionality, authority, durability and recovery remain untouched.
Nothing was approved or integrated by this worker.
