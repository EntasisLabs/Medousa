# Documentation governance

How we keep the Operator’s Guide honest as the product moves.

Living epic (repo): `Medousa/architecture/operators-guide-docs-epic.md`. Cursor rule: `.cursor/rules/operators-guide.mdc` when editing `guide/**`.

## Feature → doc checklist

Before merging a user-visible Home change, answer:

1. **New surface or Settings band?** → Chapter or section + `catalog.ts` entry + Welcome “What you will learn” if it is first-class.
2. **New shortcut, Spotlight command, or slash?** → Update source catalog (`keyboardShortcutsCatalog.ts` / `registry.ts` / `slashCommands.ts`) and run `npm run guide:generate`.
3. **New permission, budget, or safety control?** → [Permissions](guide:permissions-budgets) + [Settings reference](guide:settings-reference).
4. **Platform-specific?** → [Platform matrix](guide:platform-matrix).
5. **New limit or breaking rename?** → [FAQ / limits](guide:faq-limits) + [What’s new](guide:whats-new).
6. **Epic status** → Tick package checkboxes + working-log line in `operators-guide-docs-epic.md`.

## Release gating (lightweight)

| Gate | Check |
|------|--------|
| Guide pages load | `npm test -- --run src/lib/guide/loadGuide.test.ts` |
| Commands appendix fresh | `npm run guide:generate` — commit if diff |
| No inventing integrator trees | Operator UI docs stay in `src/lib/guide/`; engine HTTP docs stay in `docs/` |
| Cross-links | Prefer `guide:chapter-id`; callouts use ` ```callout ` |

## Voice

Operator-manual: procedures, failure modes, platform differences. Prefer structural chapters over lengthening every page equally.

## Ownership

| Artifact | Owner habit |
|----------|-------------|
| `src/lib/guide/pages/*.md` | Same PR as the UI when possible |
| `catalog.ts` | Required with new/renamed chapters |
| Generated `24-commands-reference.md` | Via `guide:generate` only |
| `operators-guide-docs-epic.md` | Update when a D-phase package ships |

Next: [What’s new](guide:whats-new) · [Welcome](guide:welcome).
