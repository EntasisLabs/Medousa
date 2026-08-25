# iOS embedded daemon — Phase 0 baseline

**Recorded:** 2026-08-23

**Status:** green

**Plan:** [ios-embedded-daemon-plan.md](ios-embedded-daemon-plan.md)

This is the pre-embedded-daemon baseline. It measures the existing Medousa iOS
app before the daemon's Stasis, Locus, SurrealKV, or agent-loop services are
composed in-process.

## Qualification environment

| Item | Value |
|------|-------|
| Host | Mac mini (Mac16,10), Apple M4 10-core, 16 GB |
| macOS | 26.5.2 (25F84) |
| Xcode | 26.5 (17F42) |
| iOS SDK | 26.5; deployment target 16.2 |
| Simulator | iPhone 16 Pro, iOS 18.3.1, arm64 |
| Rust | 1.98.0, `aarch64-apple-darwin` host |

## Dependency gate

- Stasis `0.9.3`, Locus Core `0.5.0`, and Locus SDK `0.3.0` resolve in the
  workspace.
- `medousa-engine`, Stasis + Locus + `locus-surreal-adapter` native, lean
  Grapheme, and Apple Keychain linkage compile and link for both
  `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
- The candidate slices are built independently so Stasis's current Grapheme
  host feature cannot mask the lean Grapheme profile through Cargo feature
  unification.
- The existing desktop library check remains green after the dependency
  upgrade.

Run the repeatable dependency gate from the repository root:

```bash
./scripts/ci/check-ios-embedded-deps.sh
```

## Pre-embedding measurements

The app was built in release mode with Xcode's simulator signing path:

```bash
cd apps/medousa-home
MEDOUSA_LIVE_ACTIVITY=1 npx tauri ios build --target aarch64-sim --ci
```

Do not add `--no-sign` to the Keychain qualification build. That path omits
Xcode's simulated application-identifier entitlement, and iOS correctly rejects
Keychain access with OSStatus `-34018`.

| Metric | Baseline |
|--------|----------|
| Release simulator `.app` disk usage | 36,460 KiB |
| Release simulator Mach-O executable | 36,664,560 bytes |
| First launch after fresh install | 212 ms Rust bootstrap; 24 ms setup |
| First launch after simulator reboot | 103 ms Rust bootstrap; 12 ms setup |
| Warm relaunch samples | 57, 59, 57 ms Rust bootstrap |
| Warm relaunch median | 57 ms Rust bootstrap; 3 ms setup |

`rust_startup_ms` measures entry into `run_home` through completion of Tauri's
Rust setup callback. It is not a visual-first-frame measurement.

## Keychain proof

The release simulator app completed five of five strict Keychain probes. Each
probe writes a random client credential directly to the OS keyring, reads and
compares it, deletes it, and verifies that it is absent. It does not use the
file fallback and never records the probe value.

The simulator smoke installs the app, launches the opt-in diagnostic, requires
a fresh receipt from the app's own cache directory, reports the startup/size
measurements, and then terminates the app without uninstalling it:

```bash
./scripts/ci/smoke-ios-phase0.sh booted
```

## Interpretation

- Phase 0 proves target compatibility and credential behavior; it ships no
  embedded daemon behavior.
- Simulator size is a comparison baseline, not an App Store download-size
  estimate.
- Physical-device startup, memory, thermal behavior, and final app-size delta
  remain Phase 4 acceptance work.
