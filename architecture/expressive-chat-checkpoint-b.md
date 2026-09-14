# Expressive Chat Checkpoint B — generated and chat media validation

> **Status:** Implementation complete; physical and live-provider validation pending
> **Implementation:** Slices 3–4
> **App version:** `medousa-home` 0.10.3
> **Prepared:** 2026-09-11

This record is the release gate between Slice 4 and Slice 5 of the
[Expressive Chat epic](expressive-chat-media-ink-liquid-epic.md). Do not begin
Slice 5 until the product owner accepts this checkpoint or records named
follow-ups.

## Automated evidence

| Gate | Result |
|------|--------|
| Full Medousa Rust library suite | ✅ 1,689 passed, 3 ignored, 0 failed |
| Tauri shell compile | ✅ Passed (`cargo check`) |
| Focused drawing/media/tool tests | ✅ 15 Home tests plus Rust drawing, lineage, and provider-error tests passed |
| Medousa Svelte/TypeScript check | ✅ 0 errors, 0 warnings |
| Full Medousa Home Vitest suite | ✅ 304 files, 1,578 tests passed |
| Python SDK generation/parity | ✅ Ruff clean; 25 tests passed |
| Strict documentation verification | ✅ Passed |

## Required matrix

| Surface | Workshop | Credential lane | Result | Notes |
|---------|----------|-----------------|--------|-------|
| Desktop | Local | OpenAI API key | ⬜ | |
| Desktop | Remote | OpenAI API key | ⬜ | |
| iPad | Local + remote | OpenAI API key | ⬜ | |
| Android | Local + remote | OpenAI API key | ⬜ | |
| Any | Any | Missing/expired key | ⬜ | Must fail honestly |
| Any | Any | ChatGPT sign-in | N/A | Backend probe is unsupported; UI does not advertise this as an Image API credential |

## Test procedure

- Draw from the composer, send it, confirm the selected vision route sees the
  PNG preview, reload history, reopen the retained ink, edit it, and resend.
- Repeat from a Home client connected to a remote workshop and verify no local
  path or client-owned storage is required.
- Generate square, portrait, landscape, and transparent outputs through an
  OpenAI API-key image profile.
- Cancel an in-flight generation, exercise a failing primary plus valid
  fallback, reconnect during a turn, and reload the finished transcript.
- Open a generated image, copy/share/download where supported, save it to the
  vault, attach it with **Refine**, and confirm the child generation records the
  parent lineage.
- Delete a disposable session and verify its drawing and generated-media bytes
  are removed while a vault-saved copy remains.

## Findings and sign-off

| Finding | Surface | Severity | Owner / follow-up | Accepted? |
|---------|---------|----------|-------------------|-----------|
| _Record during testing_ | | | | |

- Overall result: **🔄 Test required**
- Tested by: _Pending_
- Date: _Pending_
- Notes: _Pending_
