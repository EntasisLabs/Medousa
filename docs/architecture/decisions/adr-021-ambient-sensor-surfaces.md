# ADR-021 — Ambient sensor surfaces (watchOS, Meta Glasses)

> **Status:** Proposed  
> **Date:** 2026-08-23  
> **Scope:** Wearable and sensor companion clients that feed ambient context into a remote workshop  
> **Related:** [ADR-012](adr-012-medousa-anywhere-surfaces.md), [ambient-wearable-clients-plan.md](../../../architecture/ambient-wearable-clients-plan.md)

## Context

Medousa already treats phone Home as a portal into a remote workshop daemon
(“second screen, not second brain”). Product interest now includes Apple Watch
and Meta AI glasses for interactions like “what am I looking at?” and a wrist
quick-ask — devices that capture ambient context and present short answers
while all work runs on the paired daemon.

ADR-012’s two families do not describe these hosts well:

- They are not editor/vault **native host surfaces**.
- They are not Slack/Notion-style **external-agent adapters**.

Platform facts also constrain the architecture:

- **Meta Wearables Device Access Toolkit** exposes camera/mic to an **iOS or
  Android app**, not to a standalone glasses daemon client.
- **watchOS** favors URLSession HTTP and short interactive sessions; it cannot
  host Tauri/Svelte Home or run `medousa_daemon`.

The engine already supports the core data path: media upload, vision turns via
`media_refs`, and STT transcription for dictation.

## Decision

### 1. Add a third Anywhere family: ambient sensor surfaces

| Family | Members (initial) | Responsibility |
|--------|-------------------|----------------|
| Native host surfaces | VS Code, Neovim, Obsidian | Host-native context + reversible edits |
| External-agent adapters | Notion, Slack, … | Translate host agent events ↔ Medousa turns |
| **Ambient sensor surfaces** | **watchOS, Meta Glasses (via phone)** | Capture frame/utterance, start thin turns, glance/voice reply |

Ambient surfaces:

- Never own vault/filesystem authority (`workshop.kind` remains remote/portal).
- Never embed inference or the turn loop.
- Always hand advanced work back to Home on the same workshop/session.

### 2. Meta Glasses are a phone-mediated sensor, not a workshop target

Glasses integrate through Medousa Home on iOS (then Android) using Meta’s DAT.
The phone’s existing pairing credentials and active workshop are the trust and
routing path. Do not invent a separate glasses pairing ceremony or a glasses
`local` workshop.

### 3. watchOS is a native companion client, not a Home port

Ship a SwiftUI watch target that reuses workshop HTTP APIs (upload optional,
STT, interactive turn). Bootstrap credentials from the paired iOS app. Do not
port the Svelte/Tauri shell to watchOS.

### 4. Reuse interactive turn + media + STT; tag surfaces explicitly

Ambient clients set:

- `surface.channel_surface`: `home-watchos` or `meta-glasses`
- all `supports_ui_artifacts` / `supports_liquid_markdown` / `supports_browser_host` → `false`
- `host_context.source`: `watchos` or `meta-glasses` (and `resource_kind` such as
  `camera_frame` when attaching vision)

v1 voice stays **STT → text prompt**. Do not expand `media_refs` to raw audio
for the first slice.

### 5. Capture-on-ask only

No silent always-on camera pipeline into the model. Explicit user gesture
(voice, tap, raise-to-speak) starts each capture.

## Consequences

### Positive

- Clear product language: sensors and glance, not mini-Home.
- Maximum reuse of media vision, STT, pairing, and interactive streaming.
- Matches Meta’s and Apple’s actual SDK topologies.
- Keeps daemon authority and Home-first install story intact.

### Tradeoffs

- Requires native Swift (and later Kotlin) work outside `@medousa/client`.
- Meta DAT publishing and permission UX are external gates.
- watchOS networking/background limits push toward short turns and careful
  upload completion, not long-lived rich SSE UIs.
- Android glasses follow Home Android maturity.

## Code anchors

- Plan: [`architecture/ambient-wearable-clients-plan.md`](../../../architecture/ambient-wearable-clients-plan.md)
- Turn surface: `crates/medousa-types/src/daemon_api.rs` (`TurnSurfaceContext`)
- Host context: `crates/medousa-types/src/turn.rs` (`HostTurnContext`)
- Media / vision / STT: `src/media_handlers.rs`, `src/media_vision.rs`, `src/stt_handlers.rs`
- Phone portal: `apps/medousa-home/`, `docs/guides/phone-pairing.md`
