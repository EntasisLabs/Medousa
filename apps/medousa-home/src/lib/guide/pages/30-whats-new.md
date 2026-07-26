# What’s new and compatibility

This guide ships **inside the app** and tracks the Home build you are running. It is not a substitute for release notes on GitHub/downloads, but it records guide-structure milestones and compatibility expectations.

Related: [Workshops and connections](guide:workshops-connections) (App update card) · [Platform matrix](guide:platform-matrix)

## Guide milestones (2026-07)

| Train | What landed in the Operator’s Guide |
|-------|-------------------------------------|
| **D0** | Architecture, first-run/wizard truth, surface inventory |
| **D1** | Chat manual, permissions/budgets, Work board, troubleshooting |
| **D2** | Vault/recovery, browser, calendar, automations, agents, You/Map, peers/channels, views, runtime, MCP/packages, Liquid |
| **D3** | Generated commands appendix, settings reference, platform matrix, data lifecycle, a11y/themes, recipes, FAQ, governance |

App version in Settings → Workshop → **App** may be ahead of or behind a specific chapter — when UI and guide disagree, trust the UI and file a doc fix.

## Compatibility expectations

| Pairing | Expectation |
|---------|-------------|
| Home ↔ engine | Newer Home may need a newer daemon for dictation, Shared mode, MCP, Versions |
| Phone ↔ host | Pair with a host on the same major train; re-pair after host rebuilds |
| Guide ↔ UI | Chapter ids are stable; file numbers may change when chapters insert |
| Themes / layouts | Named themes are layout-scoped — switching presets can change colors |

## Deprecated / renamed labels

| Old copy | Current |
|----------|---------|
| Settings → Rhythm | Preferences (Work cards, Everyday, Look) |
| Workspace (rail) | Often **Library** in nav (Spotlight may still say Workspace) |
| Recipes (scripts) | UI **Templates** |
| Specialists (surface) | UI **Agents** |

## How to check your build

1. Settings → Workshop → App version / Check for updates.
2. Workshop → Engine tile — version and tool count.
3. Spotlight → Operator’s Guide — you are reading the bundled chapters for this build.

When you ship a user-visible feature, update the guide in the same change set — [Documentation governance](guide:docs-governance).
