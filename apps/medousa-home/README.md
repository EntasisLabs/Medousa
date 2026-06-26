# Medousa (app source)

Native desktop and mobile shell for **Medousa**.

| Doc | Purpose |
|-----|---------|
| [Product README](../../README.md) | Download, features |
| [Integrator reference](../../docs/apps/medousa-home.md) | Surfaces, IPC, transport, stores |
| [Build from source](../../docs/cookbook/build-from-source.md) | Cargo, Tauri dev |
| [Mobile dev](MOBILE-DEV.md) | iPhone on Mac |
| [Roadmap](../../architecture/ROADMAP.md) | Active priorities |

Tauri v2 + SvelteKit + Skeleton UI.

## Prerequisites

- Node.js 20+ with npm
- Rust toolchain (for Tauri)
- **Released app builds** bundle and start the engine automatically.
- **Dev only:** run `medousa_daemon` on `http://127.0.0.1:7419` or set `MEDOUSA_DAEMON_URL`.

## Develop

```bash
cd apps/medousa-home
npm install
npm run tauri dev
```

## Surfaces (summary)

- **Desktop:** Chat, Work board, Library (vault + files + presentations), Workshop, Settings.
- **Mobile (≤768px):** Pulse, Work timeline, Chat, You hub (Library with Notes/Presentations tabs).

Artifact fullscreen on mobile uses safe-area chrome and a leading Close control. See [integrator doc](../../docs/apps/medousa-home.md).

Design history (archived): [`medousa-home-tauri-design.md`](../../architecture/archive/medousa-home-tauri-design.md), [`medousa-home-mobile-plan.md`](../../architecture/archive/medousa-home-mobile-plan.md).
