# Build from source

## Medousa app (Tauri)

Engine should be running (or will be started by the app):

```bash
medousa start daemon --inference   # recommended for offline dev

cd apps/medousa-home
npm install
npm run tauri dev
```

Release binary:

```bash
cd apps/medousa-home
npm run tauri build
```

Artifacts: `apps/medousa-home/src-tauri/target/release/`

Design notes: [architecture/medousa-home-tauri-design.md](../../architecture/medousa-home-tauri-design.md)

---

## Engine only (Cargo)

```bash
cd Medousa   # repo root with Cargo.toml
cargo build -p medousa --bin medousa_daemon --features embedded-inference-metal
cargo run -p medousa --bin medousa_daemon --features embedded-inference-metal -- --local-engine
```

Or install full CLI set:

```bash
./scripts/install.sh --from-source
```

---

## iPhone dev

Clone on Mac, engine on LAN, install to device:

[apps/medousa-home/MOBILE-DEV.md](../../apps/medousa-home/MOBILE-DEV.md)

---

## App repo layout

| Path | Purpose |
|------|---------|
| `apps/medousa-home/` | Medousa desktop + mobile shell |
| `src/bin/medousa_daemon.rs` | Medousa Engine |
| `src/bin/medousa.rs` | CLI launcher |
| `architecture/` | Plans & component docs |
