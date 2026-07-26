# Operator’s Guide — maintenance (project only)

**Not shipped in the in-app product guide.** This is for contributors maintaining Home.

Living epic: [operators-guide-docs-epic.md](operators-guide-docs-epic.md)  
Cursor rule: `.cursor/rules/operators-guide.mdc` (globs `guide/**`)

## Product vs project

| In-app `src/lib/guide/pages` | Repo only |
|------------------------------|-----------|
| Product voice for end users | This file, epic, Cursor rule |
| No `npm`, PR, source paths, D0–D3 | Generate script + tests |

Ban in chapter bodies unless the UI shows the word: `npm`, `repo`, `PR`, `catalog.ts`, `vitest`, `epic`, `D0`–`D3`, `Tauri`, `Stasis`, `OTel`, `ACP`, contributor/merge/CI, Cursor rule.

Prefer **workshop** over daemon in prose; if Spotlight says “Check daemon health”, quote the label once then explain in plain words.

## Feature → doc checklist

Before merging a user-visible Home change:

1. New surface or Settings band → chapter/section + `catalog.ts` + Welcome / Find answers if everyday.
2. New shortcut / Spotlight / slash → update source catalogs + `npm run guide:generate` (emit must stay product-safe).
3. New permission or safety control → permissions + settings reference chapters.
4. Platform-specific → platform matrix.
5. New limit or rename → FAQ + What’s new (product voice).
6. Epic working log when a docs package ships.

## Release gates

| Gate | Check |
|------|--------|
| Pages load | `npm test -- --run src/lib/guide/loadGuide.test.ts` |
| Commands appendix | `npm run guide:generate` — commit if tables change |
| Operator UI docs only under `apps/medousa-home/src/lib/guide/` | Integrator docs stay in `docs/` |
| Cross-links | `guide:chapter-id`; callouts use ` ```callout ` |

## Mom success test (product)

A non-technical reader using only **Start + Everyday** can:

1. Send a chat and find Voice / Stance under the box.
2. Create or open a Library note.
3. Pair a phone (or know QR is minted on the desktop).
4. Recover from Offline via Workshop status.
5. Understand Allow vs Deny without reading Runtime Controls.
