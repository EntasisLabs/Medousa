# Embedded local inference (Medousa Engine)

> **Status:** Phase 2 landed — Phase 3 next (full Gemma matrix + routing)  
> **Date:** 2026-06-07 (Gemma 4 catalog lock — June 2026 releases)  
> **Default brain:** **Gemma 4** family — hero model **Gemma 4 12B Unified** (June 3, 2026) on 16 GB+ Macs  
> **Goal:** non-devs never install Ollama. Medousa Core downloads a curated Gemma 4 build, runs it in-process, and exposes it to the existing turn pipeline. Home (desktop + iPhone) stays a portal.  
> **Related:** [first-run-and-lan-pairing-plan.md](archive/first-run-and-lan-pairing-plan.md), [component-daemon.md](component-daemon.md), [durable-turn-worker-plan.md](durable-turn-worker-plan.md)

---

## Product promise

**Download → pick “Offline brain” → model downloads once → chat.**

- No Ollama install, no Hugging Face account required for curated models.
- Power users can still use Ollama, cloud BYOK, or custom HF URLs later.
- Phone app never hosts the model — it connects to Core on the Mac (see mobile wizard).

This closes the gap called out in the first-run onboarding epic: **Screen 1 Offline path** and **Recommended managed AI** eventually share the same “Medousa-hosted model” story, but v1 ships with a **local embedded Gemma 4 engine** first (privacy, no cloud dependency).

---

## Why Gemma 4 (June 2026)

Google’s Gemma 4 line is the curated default for Medousa — not because it’s trendy, but because it matches the product shape: **strong reasoning on typical laptop hardware**, multimodal headroom for future vault/vision work, and explicit **laptop / 16 GB unified memory** targeting.

| Release | Model | Why it matters for Medousa |
|---------|-------|---------------------------|
| **Mar 31, 2026** | E2B, E4B, 26B A4B, 31B | Full family on HF; mistral.rs day-0; Apache 2.0 |
| **Jun 3, 2026** | **Gemma 4 12B Unified** | Encoder-free multimodal (text/image/audio/video), **256K context**, runs on **16 GB** unified memory — **default “Recommended” pick** |
| **Jun 5, 2026** | QAT checkpoints (Q4_0 + mobile format) | E2B footprint down to **~1 GB** — tier A without a junk model |

Sources: [Google Gemma 4 12B guide](https://developers.googleblog.com/en/gemma-4-12b-the-developer-guide/), [Gemma releases](https://ai.google.dev/gemma/docs/releases), [HF Gemma 4 blog](https://huggingface.co/blog/gemma4), [QAT announcement](https://blog.google/innovation-and-ai/technology/developers-tools/quantization-aware-training-gemma-4/).

**Catalog principle:** every tier gets a **Gemma 4** variant first; non-Gemma entries are fallbacks only (legacy hardware, user override).

---

## Where this fits today

```
┌─────────────────────────────────────────────────────────────┐
│  Medousa Home (desktop / iOS) — portal only                 │
│  wizard · chat · settings · pairing                         │
└───────────────────────────┬─────────────────────────────────┘
                            │ HTTP/SSE :7419
┌───────────────────────────▼─────────────────────────────────┐
│  medousa_daemon (Medousa Core)                              │
│  Stasis · turn worker · identity · vault · pairing          │
│  GenaiChatClient ──► provider adapter (today: HTTP)         │
│       ▲                                                     │
│       │  NEW: embedded engine (in-process or loopback)      │
│  ┌────┴──────────────────────────────────────────────┐      │
│  │  Local inference runtime (mistral.rs / Candle)     │      │
│  │  model store · download manager · hardware tier  │      │
│  └───────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────┘
         optional external: Ollama :11434 · OpenAI · Anthropic
```

**Integration point (minimal churn):** keep `GenaiChatClient` as the single chat surface. The embedded engine exposes an **OpenAI-compatible** chat/completions API on a **loopback port** (e.g. `127.0.0.1:7421`) or implements a thin custom genai adapter if we outgrow that. Wizard + `tui_defaults.json` set:

```json
{
  "provider": "medousa-local",
  "model": "gemma-4-12b-it",
  "baseUrl": "http://127.0.0.1:7421/v1"
}
```

Stage routing (`orchestrator`, `chunker`, …) can pin cheap models on low tiers and one “voice” model for `final_response` — reuses existing matrix routing from the durable turn worker work.

---

## Rust library choice

### Recommendation: **mistral.rs** (primary)

| Criterion | mistral.rs | Raw **Candle** | **llama-cpp-2** (llama.cpp bindings) | **tch-rs** (LibTorch) |
|-----------|------------|----------------|--------------------------------------|------------------------|
| Pure Rust stack | Yes (on Candle) | Yes | No (C++ core) | No (LibTorch) |
| GGUF + quant (Q4/Q8) | Yes | Yes (lower level) | Best-in-class | Awkward |
| HF Hub download | Yes | via `hf-hub` | Manual / side tools | Manual |
| OpenAI-compatible server | Built-in | Roll your own | via llama-server | Roll your own |
| Metal (Apple Silicon) | Yes | Yes | Yes (very mature) | CUDA-focused |
| Async / daemon-friendly | Yes | Build yourself | Sync C API wrappers | Heavy |
| Binary size / deploy | Moderate | Smaller if custom | + native lib | Huge |
| Medousa fit | **Best** — engine, not research framework | Too low-level for product | Good fallback backend | Wrong tool |

**Decision:** depend on **`mistralrs`** inside `medousa_daemon` (feature-gated: `embedded-inference`). mistral.rs has **day-0 Gemma 4** support (text + image + audio + video) via safetensors / **UQFF** / in-situ quant — see [mistral.rs Gemma 4 docs](https://github.com/EricLBuehler/mistral.rs/blob/master/docs/GEMMA4.md).

**Gemma 4 weight format (important):**

| Format | Status for Gemma 4 | Medousa use |
|--------|-------------------|-------------|
| **UQFF / ISQ + HF safetensors** | Supported in mistral.rs | **Primary curated path** — e.g. `mistralrs-community/gemma-4-E4B-it-UQFF` |
| **Google QAT Q4_0** | HF checkpoints Jun 2026 | Tier A mobile-QAT E2B (~1 GB) when UQFF mirror ready |
| **GGUF (unsloth, llama.cpp)** | llama.cpp ✅; mistral.rs GGUF `gemma4` arch **in progress** ([#2171](https://github.com/EricLBuehler/mistral.rs/issues/2171)) | Secondary mirror in manifest; optional **llama.cpp loopback** fallback until mistral.rs GGUF lands |

Use **`hf-hub`** for curated downloads. Treat **Candle** as transitive only.

**Optional Phase 2+ fallback:** `llama-cpp-2` loopback for GGUF catalog entries if a specific Gemma 4 GGUF beats UQFF on Apple Silicon in benchmarks — one `LocalEngine` trait, swappable backend.

**Reject for v1:** tch-rs/LibTorch (deploy weight), ONNX Runtime (different ecosystem), Python subprocess to vLLM (ops burden).

### References

- [mistral.rs](https://github.com/EricLBuehler/mistral.rs) — Candle-based inference, GGUF, ISQ, OpenAI API, Metal/CUDA
- [Candle](https://github.com/huggingface/candle) — HF’s Rust ML core
- [hf-hub](https://github.com/huggingface/hf-hub) — authenticated + cached model file downloads
- Existing product wiring: `GenaiChatClient` in `medousa/src/lib.rs`, provider resolution in `resolve_llm_*`

---

## Curated model catalog (Gemma 4 first)

non-devs never browse Hugging Face. We ship a **signed manifest** in the repo (updated with Medousa releases). **All entries are Gemma 4** at launch; IDs match wizard copy and `tui_defaults.model`.

### Launch catalog (v1)

| Catalog ID | Display name | Tier | HF / engine source | Download (~) | RAM (~) | Role |
|------------|--------------|------|-------------------|--------------|---------|------|
| **`gemma-4-e2b-it-qat`** | Gemma 4 E2B — light | A | `google/gemma-4-E2B-it` QAT mobile / UQFF 4-bit | **~1–3 GB** | ~2 GB | Minimum Mac; fast roles |
| **`gemma-4-e4b-it`** | Gemma 4 E4B — balanced | B | `google/gemma-4-E4B-it` or `mistralrs-community/gemma-4-E4B-it-UQFF` | **~5 GB** | ~6 GB | Default on 8–16 GB |
| **`gemma-4-12b-it`** | **Gemma 4 12B — recommended** | C+ | `google/gemma-4-12B-it` (Unified, Jun 2026) | **~7–9 GB** Q4 class | ~12–16 GB | **Hero model** — wizard “Recommended / Offline” |
| **`gemma-4-26b-a4b-it`** | Gemma 4 26B MoE — deep | D/E | `google/gemma-4-26B-A4B-it` UQFF / Q4 GGUF | **~17 GB** | ~20 GB+ | Optional “think harder” toggle |

Official instruct variants: `-it` suffix. Base weights are not offered in the Home catalog.

### Example manifest entry (hero model)

```json
{
  "catalogVersion": "2",
  "familyDefault": "gemma-4",
  "models": [
    {
      "id": "gemma-4-12b-it",
      "displayName": "Gemma 4 — Recommended",
      "family": "gemma-4",
      "variant": "12b-unified",
      "tierMin": "C",
      "tierRecommended": true,
      "format": "uqff",
      "source": "huggingface",
      "repo": "google/gemma-4-12B-it",
      "engine": "mistralrs",
      "engineArgs": { "fromUqff": 4 },
      "fallback": {
        "format": "gguf",
        "repo": "unsloth/gemma-4-12b-it-GGUF",
        "file": "gemma-4-12b-it-Q4_K_M.gguf",
        "backend": "llama.cpp"
      },
      "sha256": "...",
      "sizeBytes": 7500000000,
      "contextLength": 262144,
      "ramEstimateMb": 14000,
      "modalities": ["text", "image", "audio", "video"],
      "license": "Apache-2.0",
      "tags": ["recommended", "offline", "hero", "jun-2026-unified"]
    }
  ]
}
```

**Catalog rules**

| Rule | Rationale |
|------|-----------|
| **Gemma 4 only in Home catalog** | One family, one voice; tier = size variant not brand hop |
| **`-it` instruct weights only** | Chat out of the box |
| **UQFF / QAT first, GGUF fallback** | mistral.rs native path; GGUF via llama.cpp until gemma4 GGUF in mistral.rs |
| **≤4 curated SKUs at launch** | E2B, E4B, 12B hero, 26B optional |
| **SHA256 pinned per file** | Reproducible downloads |
| **12B Unified = default when tier ≥ C** | Matches Google’s 16 GB laptop story |
| **No auto-pull `main`** | Manifest version bumps ship with Medousa releases |

**On-disk layout**

```
~/.local/share/medousa/
  models/
    manifest.json          # installed subset + paths
    cache/
      gemma-4-12b-it/
        weights/               # UQFF shards or GGUF
  hardware-profile.json    # last probe + tier
```

---

## Hardware tiers (capability buckets)

Probe once at wizard / first offline path selection; re-probe on “model feels slow” or manual refresh in Settings → Voice.

### Signals

| Signal | Source (Rust) | Notes |
|--------|---------------|-------|
| Total RAM | `sysinfo` | Primary gate |
| Available RAM | `sysinfo` | Avoid recommending when disk swap-heavy |
| CPU cores / arch | `sysinfo` | ARM vs x86; AVX2 on Linux |
| GPU backend | mistral.rs / Candle features | `metal`, `cuda`, or CPU-only |
| VRAM / unified memory hint | platform-specific | Apple: treat unified RAM; discrete GPU: `nvml` later |
| Free disk | `fs2` / stat | Need 2× model size headroom for download |
| Optional: battery / thermal | mobile irrelevant on Mac host | defer |

### Tier table (Gemma 4 variants)

| Tier | Typical hardware | Default catalog pick | Notes |
|------|------------------|----------------------|-------|
| **A** — Minimal | 8 GB, old Intel | **Gemma 4 E2B-it** (QAT ~1 GB or Q4 ~3 GB) | Short context OK for chunker roles; hero chat still works |
| **B** — Everyday | 8–16 GB, M1 8 GB | **Gemma 4 E4B-it** | Wizard default when 12B won’t fit |
| **C** — Comfortable | 16 GB unified (M1 Pro / M2 / M3) | **Gemma 4 12B Unified-it** | **Recommended** — encoder-free, multimodal-ready |
| **D** — Enthusiast | 24–32 GB or discrete 8 GB VRAM | **Gemma 4 12B** Q8 or **26B A4B-it** Q4 | “Quality” toggle in Settings |
| **E** — Workstation | 48 GB+ / 16 GB VRAM | **Gemma 4 26B A4B-it** or 31B | Advanced catalog only |

**Scoring (v1 heuristic — tuned for Gemma 4 12B gate at 16 GB)**

```
tier = A
if total_ram_gb >= 8 and free_disk_gb >= 4: tier = A   # E2B
if total_ram_gb >= 12: tier = B                          # E4B
if total_ram_gb >= 16: tier = C                          # 12B Unified ← hero
if total_ram_gb >= 24 and (metal or cuda): tier = D
if total_ram_gb >= 48: tier = E
never recommend 12B if available_ram_mb < 12000
never recommend 26B if available_ram_mb < 20000
```

Store result in `hardware-profile.json` and expose via `GET /v1/runtime/hardware` (daemon) + Tauri IPC for wizard.

**UX copy (wizard Screen 1 — Offline / Recommended)**

> “Your Mac is a **16 GB — Comfortable** brain. We recommend **Gemma 4** (~7 GB download) — Google’s latest unified model, built for laptops like yours.”

Subline for tier B: “We’ll start with **Gemma 4 E4B** — same family, lighter download.”

Buttons: **Download Gemma 4** · **Pick another size** · **Use cloud key instead**

---

## Daemon modules (new)

| Module | Path (proposed) | Responsibility |
|--------|-----------------|----------------|
| `hardware_probe` | `src/local_inference/hardware.rs` | Tier scoring, persist profile |
| `model_catalog` | `src/local_inference/catalog.rs` | Load signed manifest, filter by tier |
| `model_store` | `src/local_inference/store.rs` | Download, verify SHA256, resume, delete |
| `engine` | `src/local_inference/engine.rs` | mistral.rs runner, load/unload, stream tokens |
| `engine_http` | `src/local_inference/http.rs` | Loopback OpenAI shim (or mount on axum `:7419/v1/local/...`) |
| `handlers` | `src/local_inference_handlers.rs` | `GET /models/catalog`, `POST /models/download`, progress SSE |

**Feature flag:** `embedded-inference` in `medousa/Cargo.toml` — default **off** in dev until Phase 1 lands; **on** in release app builds.

**Idle RAM discipline:** engine **not loaded** until first chat or explicit preload; target remains ~30 MB daemon idle without weights mapped.

---

## API surface (daemon)

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/v1/local/hardware` | Tier, RAM, GPU, recommended model ids |
| `GET` | `/v1/local/catalog` | Filtered manifest for this tier |
| `GET` | `/v1/local/models` | Installed models + status |
| `POST` | `/v1/local/models/download` | `{ "modelId": "..." }` — async job |
| `GET` | `/v1/local/models/download/{jobId}` | Progress `{ percent, bytes, phase }` |
| `DELETE` | `/v1/local/models/{modelId}` | Free disk |
| `POST` | `/v1/local/engine/load` | Warm model (wizard completion) |
| `GET` | `/v1/local/engine/status` | `{ loaded, modelId, tokensPerSecEstimate }` |

Internal chat: `http://127.0.0.1:7421/v1/chat/completions` (mistral.rs OpenAI server) — not exposed off localhost.

**Tauri events (Home):** reuse `model_download_progress` from first-run onboarding spec; add `local_engine_ready`.

---

## Wizard & Settings integration

| Surface | Change |
|---------|--------|
| **Desktop Screen 1 — Offline / Recommended** | Hardware probe → **Gemma 4** tier pick (12B hero when ≥16 GB) → download → `medousa-local` + `gemma-4-*-it` |
| **Desktop Screen 1 — BYOM** | Keep Ollama detect + cloud keys; “Medousa offline model” card when catalog available |
| **Mobile Screen 1** | Unchanged — connect to Mac; Mac owns model |
| **Settings → Voice** | Show tier, installed local model, download/remove, re-run hardware probe |
| **TUI / CLI** | `medousa start daemon --inference` (dev); `medousa models …` — not the Home app path |

Cross-ref: [first-run-and-lan-pairing-plan.md](archive/first-run-and-lan-pairing-plan.md) Phase C/E offline + recommended paths.

---

## Implementation phases

### Phase 0 — Probe + catalog skeleton (~1 week)

- [x] `hardware_probe` + `hardware-profile.json`
- [x] Curated `catalog/v2.json` — **Gemma 4 E2B / E4B / 12B / 26B A4B** entries with pinned SHAs
- [x] Daemon HTTP: `GET /v1/local/hardware`, `GET /v1/local/catalog` (static JSON)
- [x] Unit tests for tier scoring with mocked sysinfo

**Exit:** `curl localhost:7419/v1/local/hardware` returns tier B on a 16 GB M2.

### Phase 1 — Engine spike (~2 weeks)

- [x] `embedded-inference` feature + mistral.rs (pin ≥0.8.x with Gemma 4)
- [x] Load **`google/gemma-4-E4B-it`** or **`gemma-4-12B-it`** via ISQ/UQFF in dev
- [x] Loopback OpenAI server on `:7421`; wire `medousa-local` through `GenaiChatClient`
- [ ] One interactive turn E2E on CPU (Metal on macOS if easy) — manual smoke with `--local-engine`

**Exit:** Desktop wizard offline path can chat without Ollama on a dev machine.

### Phase 2 — Download manager + wizard UX (~2 weeks)

- [x] Async download job + progress SSE / Tauri event
- [x] SHA256 verify + resume
- [x] Wizard Screen 1 offline card wired end-to-end
- [x] Settings → Voice local model panel

**Exit:** Fresh VM: Offline → **Gemma 4** download → chat, no terminal.

### Phase 3 — Full Gemma 4 matrix + routing (~2 weeks)

- [ ] All four tier SKUs + 26B optional
- [ ] Stage routing: E2B on chunker/extractor for tier A/B; **12B on orchestrator + final_response**
- [ ] Uninstall + disk quota warning

**Exit:** 8 GB Mac gets E2B only; 16 GB Mac gets **12B Unified** by default.

### Phase 4 — Polish & ops (~1 week)

- [ ] Engine unload on idle timeout (configurable)
- [x] Doctor: `medousa doctor --local-engine` (+ `medousa models` power-user commands)
- [ ] Release notes + manifest update process when Google ships Gemma 4 point releases
- [ ] Benchmark: mistral.rs UQFF vs llama.cpp GGUF for 12B on M-series; pick default backend per platform

---

## Non-goals (v1)

- On-phone embedded inference (separate “Pocket” product later)
- Merging daemon into Home single binary (SDK/sidecar — post-v1)
- Arbitrary HF model IDs from Home UI
- Fine-tuning or LoRA training
- Replacing Ollama for power users who already have it

---

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| 8 GB Mac, 12B too heavy | Tier probe caps at E4B/E2B; never offer 12B below 16 GB RAM |
| Gemma 4 GGUF in mistral.rs immature | Primary path = UQFF/safetensors; GGUF + llama.cpp fallback in manifest |
| Google license / HF gating | Accept Gemma license once in wizard; cache acceptance in `wizard.json` |
| Download size (12B ~7 GB) | Show size upfront; Wi‑Fi-only toggle; resume; E4B fallback offered inline |

---

## Open questions

1. **Single loopback port vs in-process trait** — loopback first for speed of integration; in-process genai adapter if latency matters for multi-role routing.
2. **Manifest signing** — ed25519 ship list vs git-trusted for v1?
4. **Gemma license gate** — single wizard beat (“Google Gemma terms”) before first download?
5. **Multimodal in v1** — ship text-only inference first, enable image/audio when vault vision ships?

---

*Next step: Phase 3 — stage routing matrix + tier-sized model defaults across turn pipeline.*
