# Medousa Inference Runtime — cold, adaptive, small-model first

**Status:** proposed active epic

**Working name:** MIR (Medousa Inference Runtime)

**Last updated:** 2026-08-01

**Research ledger:** [inference-research-index.md](inference-research-index.md)

**Related plans:** [embedded-local-inference-plan.md](embedded-local-inference-plan.md), [inference-profiles-and-model-catalog-plan.md](inference-profiles-and-model-catalog-plan.md)

This epic replaces the runtime and memory-management portions of the original
embedded inference plan. Its product promise, catalog, and download work remain
useful. MIR changes the engine from “a model server we spawn” into a measured,
resource-governed inference subsystem.

---

## Executive decision

Medousa will own its **inference control plane**:

- lifecycle and process supervision;
- resource admission and memory-pressure response;
- model/engine recipe selection;
- interactive scheduling and cancellation;
- KV and prefix-cache policy;
- structured generation and tool boundaries;
- workload-aware model routing;
- model artifact verification;
- quality/performance evaluation; and
- optional adaptation and distillation pipelines.

Medousa will initially use replaceable execution backends rather than writing a
complete tensor engine first. `mistral.rs`, `llama.cpp`, MLX/MLX Swift,
TensorRT-LLM, qualified ROCm/HIP engines, and Core ML/ExecuTorch are candidates,
not architectural authorities. A backend earns a default through Medousa
measurements for a specific OS, accelerator, driver/runtime, model, and recipe.

Metal, CUDA, and ROCm/HIP are first-class execution lanes. CPU is the universal
safety baseline; Vulkan, DirectML, SYCL/OpenVINO, and other delegates are
qualified portability lanes where a vendor-native path is absent or loses.
Medousa will not collapse these into a lowest-common-denominator engine, nor let
an Apple-, NVIDIA-, or AMD-specific optimization leak into the control-plane
contract.

MIR also does not assume that autoregressive next-token transformers are the
final form of local intelligence. Diffusion/canvas generation, speculative and
multi-token prediction, recurrent models, activation steering, learned latent
routing, vector-symbolic control/memory, and new quantized geometries remain
valid research lanes. They earn product status through the same safety, quality,
latency, memory, energy, lifecycle, and portability evidence as conventional
engines.

We will write or upstream kernels only after profiling identifies a stable,
material gap that engine configuration, format choice, or an upstream backend
cannot solve. Owning the system means being free to do that later without
rewriting Home, the daemon, the catalog, or the turn runtime.

## Product promise

> Local Brain is instant when it can be, patient when it must be, and invisible
> to the rest of the computer when it is not being used.

For the user this means:

- installing or selecting a local model does not allocate model memory;
- the first local request clearly warms the selected model;
- Medousa chooses safe defaults for this machine and current memory pressure;
- small models get focused work, tools, constraints, and context that let them
  perform above their general-chat weight class;
- long work never silently consumes the machine's last usable memory;
- cancellation actually stops work;
- idle memory returns to the operating system; and
- advanced users can inspect and override recipes without needing to understand
  inference internals.

## Non-goals

- Training a frontier foundation model from scratch.
- Optimizing for datacenter multi-tenant throughput before interactive desktop
  behavior is excellent.
- Maintaining a fork of every inference engine.
- Treating today's autoregressive engine APIs as a permanent generation model.
- Advertising a model's maximum context as a safe default context.
- Keeping a model resident merely because it is installed or selected.
- Downloading or executing unpinned remote model code.
- Hiding quality regressions behind aggregate tokens-per-second numbers.

---

## Why the present runtime is not lazy

The current code has a useful process boundary but collapses too many states:

1. `WizardWelcomeScreen.svelte` confirms Local Brain and starts model prep.
2. `wizard.svelte.ts::runBrainModelPrep` downloads and immediately invokes
   `local_inference_spawn_engine`.
3. `workshop_runtime::ensure_local_engine(..., true)` can start both the core
   daemon and Local Brain.
4. `spawn_local_brain` launches `medousa_local --model-id ...` or
   `--load-recommended`.
5. `medousa_local` calls `LocalEngineRuntime::load` before binding HTTP. Port
   readiness therefore means weights and runtime are already resident.
6. The `mistral.rs 0.8.1` wrapper uses default maximum sequence and batch
   parameters and enables in-situ quantization without a pre-quantized artifact.
7. There is no public unload command, idle policy, memory admission, memory
   pressure response, or trustworthy resident-memory status.

The existing `loaded: bool` cannot distinguish package installed, model on disk,
worker starting, weights loading, ready, busy, unloading, or a dead process with
stale PID/bind state. The process boundary is worth keeping; its lifecycle
contract is not.

## Immediate memory-risk hypotheses

These must be reproduced, not treated as a declaration of the kernel-panic root
cause:

1. **Unbounded context recipe.** Engine defaults may reserve excessive KV or
   workspace capacity for large-context models.
2. **In-situ quantization peak.** Source weights, quantized output, and conversion
   workspace may coexist during load.
3. **Unified-memory overcommit.** Metal, model mappings, the app, and the OS share
   one pool; a nominally fitting model can still drive swap/watchdog failure.
4. **No eviction.** The child remains resident indefinitely.
5. **No generation limits.** Context, output, concurrency, and scratch buffers
   are not governed by one admission decision.
6. **Underspecified readiness.** A reachable port does not validate model
   identity, recipe, worker generation, or resource state.

MIR-0 ranks these with measurement before an engine default changes.

---

## The workload we optimize

Desktop Medousa is not a public inference server. Its normal shape is one human,
one foreground answer, bursts separated by reading/tool use, repeated prefixes,
high cancellation value, tiny routing tasks mixed with deep responses, and
long-lived agent turns that pause on tools.

Metric priority:

1. machine safety and reliable memory reclamation;
2. task quality and tool correctness;
3. time to first visible token;
4. p95 inter-token latency and absence of stalls;
5. cancellation latency;
6. warm/cold energy cost;
7. steady decode speed;
8. multi-request throughput.

Continuous batching and PagedAttention may help, but datacenter throughput is
not automatically a desktop win.

---

## Quantitative resource model

Every recipe provides a conservative estimate and the worker reports actuals.

```text
weight_bytes ~= parameters * effective_bits_per_weight / 8
             + scales_and_zero_points
             + tensor_metadata

kv_bytes ~= 2
         * layers
         * kv_heads
         * head_dim
         * cached_tokens
         * bytes_per_kv_element
         * live_sequences

peak ~= resident_weights
     + conversion_or_dequantization_peak
     + KV_and_prefix_caches
     + graph_and_kernel_workspaces
     + tokenizer_and_request_buffers
     + backend_allocator_slack
     + draft_or_verifier_models
```

GQA/MQA reduce `kv_heads`; sliding/rotating windows cap cached tokens at a
quality cost. Paged storage reduces fragmentation, not logical KV size. For MoE,
total resident weights and active parameters per token are separate numbers.

Memory topology is part of the recipe:

- Apple silicon uses unified memory. MIR tracks process physical footprint,
  available memory, compression/swap, Metal's recommended working set, and
  backend-reported allocations because any one counter can under-report pressure.
- NVIDIA CUDA and discrete AMD ROCm devices use separate VRAM plus host RAM.
  MIR tracks free/used/total VRAM, per-process allocations, allocator reserve,
  workspace peaks, pinned host buffers, PCIe transfers, host pressure, and
  fragmentation. A model fitting in VRAM does not prove the host is safe.
- AMD APUs and other unified/shared-memory GPUs require topology-aware accounting
  rather than treating an advertised VRAM carve-out as independent capacity.
- Windows WDDM can budget and evict GPU memory. Native Windows recipes include
  the current process/device budget and are not admitted from physical VRAM
  alone.

### Initial admission envelope

```text
estimated_peak <= min(
  backend_device_budget - device_reserve,
  currently_available_host_memory - system_reserve,
  tier_recipe_cap
)
```

For CPU and fully unified-memory paths, `backend_device_budget` resolves to the
host/unified-memory envelope rather than inventing a separate VRAM pool.

Until calibrated, `system_reserve` is the greater of 4 GiB or 25% of physical
RAM. Activation is also rejected when swap is already stressed or pressure is
critical. The governor may shrink context, lower validated KV precision, remove
a draft/cache, select a smaller model, use a configured remote fallback, or
refuse before allocation. It must never “try and see” with the whole machine.

---

## Target architecture

```text
Home / CLI / SDK
       |
       v
medousa_daemon
  InferenceCoordinator
    |- capability/workload router
    |- recipe resolver
    |- ResourceGovernor
    |- CachePolicy
    |- InteractiveScheduler
    |
    v
LocalWorkerSupervisor
    |- versioned control protocol
    |- process generation and identity
    |- spawn / ready / cancel / drain / terminate
    |
    +--> portable llama.cpp worker (CPU/Metal/CUDA/HIP/Vulkan)
    +--> CUDA worker (NVIDIA-specialized candidate)
    +--> ROCm/HIP worker (AMD-specialized candidate)
    +--> MLX / native Metal worker (Apple candidate)
    +--> Core ML worker (curated Apple lane)
    +--> mistral.rs worker (qualified device classes)
    +--> future native MIR worker
```

The daemon owns policy but never model tensors. Backends run in disposable child
processes; process termination is the final memory-reclamation primitive.

### `InferenceCoordinator`

- Accepts capability requests rather than assuming one global chat model.
- Chooses local/remote according to explicit profiles and fallbacks.
- Resolves a versioned `InferenceRecipe`.
- Obtains an admission decision and worker lease.
- Streams normalized events and propagates cancellation.
- Records non-content performance and failure measurements.

### `ResourceGovernor`

- Probes memory, swap, CPU/accelerator, OS, working-set limits, and available
  thermal/battery hints.
- Predicts steady and peak usage from artifact/recipe metadata.
- Reserves memory atomically so concurrent activation cannot double allocate.
- Samples load and generation, responds to soft/urgent/critical pressure, and
  learns conservative observed peaks per hardware and recipe.
- Never silently changes model/context/quality after generation begins.

### `LocalWorkerSupervisor`

- Uses worker generation IDs, not “port reachable.”
- Verifies package version and artifact digest.
- Initially permits one heavyweight worker.
- Supports drain, bounded unload, forced termination, and stale-state cleanup.
- Separates process startup from model readiness.

### Backend contract

```text
probe_capabilities
estimate_resources
load(recipe, artifact)
warm(recipe)
generate(request_stream)
cancel(request_id)
cache_status
trim_caches(target_bytes)
unload
metrics
shutdown
```

Backend knobs stay inside recipes and do not leak into normal chat/provider APIs.

### Open research, hard promotion gates

MIR deliberately separates exploration from product promotion. An experiment
may begin on one model, engine, or accelerator when that is the fastest way to
learn. It does not need premature cross-platform implementation. Promotion to a
catalog default requires:

1. a falsifiable mechanism and smallest useful ablation;
2. a quality-equivalent conventional baseline;
3. full steady and peak resource accounting;
4. interactive latency, cancellation, and lifecycle measurements;
5. explicit supported platform/device recipes and safe fallbacks;
6. stable request/output semantics through the worker protocol; and
7. a reversible recipe flag with the negative result preserved if it loses.

“Industry standard” is evidence of maturity, not proof of optimality. Likewise,
novelty is permission to test, not permission to ship.

### Platform and package contract

The coordinator, worker protocol, lifecycle states, request semantics, cache
identity, and metrics schema are shared. Execution packages are optional and
platform-specific:

```text
os + architecture + accelerator vendor + device architecture
+ driver/runtime compatibility + engine build + artifact/format
-> versioned InferenceRecipe
```

The capability probe records Apple GPU family, NVIDIA compute capability, AMD
`gfx` target, memory topology, driver/runtime versions, and supported kernel
features. The resolver selects only catalog recipes that match those facts.
CUDA and HIP libraries are never bundled into the core daemon or loaded on a
machine that did not select the corresponding optional package.

Linux and Windows are separate compatibility targets even when both expose CUDA
or HIP. WSL/container serving is an explicit advanced recipe, not a hidden
dependency of the native desktop app. Every specialized package has a tested
CPU or portable-GPU fallback and uses the same unload-by-process-exit guarantee.

---

## Lifecycle contract

```text
Unavailable -> NotInstalled -> Cold -> Downloading -> Cold
Cold -> Admitting -> StartingWorker -> Loading -> Warming -> Ready
Ready -> Busy -> Ready -> Draining -> Unloading -> Cold
```

`Failed` and `Cancelled` are legal from every active phase and end in a known
`Cold`, `Ready`, or `Unavailable` state.

Required invariants:

- `Cold` means no heavyweight worker and no model-resident memory.
- `Ready` identifies exact artifact digest and recipe revision.
- Activation is single-flight.
- A different model never reuses a merely reachable stale worker.
- Download, daemon startup, and provider selection never imply activation.
- An actual local inference request is the default activation boundary.
- Cancellation during an uncancellable load terminates the worker.
- `Unloading -> Cold` occurs only after process exit and channel closure.

Initial idle policy keeps a worker for five minutes after an interactive request,
trims disposable caches first, and unloads immediately on provider switch,
explicit unload, app/core termination, logout, sleep, or critical pressure. It
does not auto-reload on wake. The timeout is policy, not a backend constant.

---

## Engine strategy

| Backend | Strength | Risk | Initial role |
|---|---|---|---|
| mistral.rs | Rust, broad models/formats, modern Metal/CUDA/cache/scheduler upstream | Our pinned wrapper is old and unsafe; broad surface; no complete AMD lane | Upgrade as a measured candidate on qualified Metal/CUDA devices, not assumed winner |
| llama.cpp | Mature GGUF, explicit controls, CPU/Metal/CUDA/HIP/Vulkan/SYCL support, and hybrid offload | Fast-moving C/C++ API; backend kernel/model support differs | Required cross-platform baseline and first native Windows/Linux package |
| TensorRT-LLM | NVIDIA-specialized kernels, quantization, paged KV, and speculative features | NVIDIA-only; plan/build/package complexity; Linux/server bias; rapid compatibility movement | CUDA performance-ceiling laboratory, initially Linux; native Windows only if upstream support and product measurements qualify it |
| vLLM / SGLang | Mature CUDA and ROCm serving techniques, paged/prefix caches, structured and speculative execution | Linux/server and throughput orientation; Python/container footprint; unload and batch-one cost | Research oracle and performance-ceiling lab, not an assumed desktop dependency |
| DiffusionGemma / diffusion worker | Parallel bidirectional canvas refinement targets low-batch accelerator utilization and non-linear generation | Different preview/commit UX, lower documented quality than autoregressive Gemma 4, 26B resident footprint, immature backend coverage | Alternative-generation laboratory for code infill, editing, structured blocks, and other lanes that benefit from whole-span revision |
| MLX / MLX Swift | Apple-native unified memory, compilation, quant ecosystem | Apple-only and packaging/conversion work | Mac performance-ceiling candidate; productize via Swift/C++ worker |
| BaseRT / Uzu | Native Metal/MPSGraph designs aimed directly at Apple inference; recent results suggest framework/dispatch overhead matters most for small models | Very new; BaseRT's engine binary is proprietary even though its tooling/bindings are Apache-2.0; Uzu is MIT; both need lifecycle and independent reproduction | Benchmark both; treat BaseRT as optional/proprietary evidence and Uzu as the open integration/source-study candidate |
| Core ML | OS compilation, stateful KV, GPU/Neural Engine low-bit paths | Curated fixed graphs and OS fragmentation | Focused 1–4B reflex/worker and mobile research lane |
| ExecuTorch | Portable edge delegation across accelerators | Export and operator gaps | Later mobile/NPU distribution lane |
| ONNX Runtime / DirectML / OpenVINO | Broad provider/delegate model across Windows and Intel hardware | LLM operator, cache, quantization, and model-conversion gaps | Curated-model portability and fallback research lane |
| Candle/custom kernels | Maximum control and Rust compatibility | Highest correctness and maintenance cost across three accelerator ecosystems | Profiling-driven experiments only |

There is no global best engine. Recipes are keyed by platform, OS, chip family,
driver/runtime, memory topology/tier, artifact, and workload. The same PC may
use different engines for a 3B text worker and a multimodal model. Provider
settings remain unchanged.

### Custom kernel gate

A Medousa Metal, CUDA, or HIP kernel requires a reproducible top-three hotspot,
exhausted engine/format options, no acceptable upstream implementation, a stable
correctness oracle, at least two testable device generations, material p50/p95
product gain, and a maintained fallback. Likely narrow targets are fused
dequantize-matmul, decode sampling, KV quantization, or grammar-mask
application—not “rewrite attention” as the first task.

---

## Making small models exceptional

The hypothesis is not that a 2B model answers everything:

> A small model can feel much larger when given the right bounded task, compact
> tools, reusable prefix state, constrained output, retrieval, execution
> feedback, and escalation path.

### Capability lanes

| Lane | Work | Experimental target |
|---|---|---|
| **Reflex** | intent, titles, classification, extraction, tool shortlist, schema repair | 0.5–2B, tiny context, constrained output |
| **Worker** | normal chat, tools, docs, code explanation/edit planning | 2–5B, compact retrieved context, strong instruction/tool tuning |
| **Deep** | hard reasoning, synthesis, complex coding | best safe local model or explicit remote fallback |
| **Vision/audio** | perception and transcription | capability-specific profile, not forced into text worker |
| **Embed/rerank** | retrieval/context selection | specialized small encoders with separate lifecycle |

Parameter ranges are experiment spaces, not catalog commitments.

### Context compiler

Small models are disproportionately harmed by irrelevant context. Medousa will
select relevant tools, emit compact canonical schemas, retrieve narrow evidence,
separate older summaries from recent verbatim turns, use typed scratch state,
reuse safe stable-prefix KV, and enforce request token budgets. It reports token
cost by identity, tools, memory, retrieved evidence, history, and user input.

### Constrained tools

- Compile JSON Schema/tool signatures into grammar constraints.
- Cache grammars by schema digest.
- Mask impossible tokens during decode.
- Validate arguments against the live registry/environment.
- Measure syntax validity, semantic correctness, hallucinated tools, and retry
  count separately.

Syntax constraints do not prove a referenced resource exists; Medousa remains
authoritative.

### Retrieval and execution feedback

Retrieve exact APIs/files/messages, use deterministic parsers and calculators,
compile/test code with bounded diagnostics, treat tool output as evidence rather
than instructions, keep high-impact actions behind policy, and escalate when
validation fails. Small models should not memorize facts Medousa can safely
look up or execute.

### Adapter packs and distillation

Future code/messaging/vault adapters require base digest compatibility, license/
provenance, load and memory measurements, full regression evals, adapter-aware
cache keys, and independent unload. Training may use verified tool traces,
execution-checked code, filtered synthetic alternatives, curriculum SFT,
on-policy distillation, and failure-focused repairs. User content is excluded
without separate informed opt-in. Every datum carries provenance, license,
generator, verifier, and deduplication metadata.

---

## Quantization policy

Quantization is a recipe with a quality record, not a filename suffix.

1. Prefer pre-quantized/reproducibly converted artifacts over foreground ISQ.
2. Measure prefill and decode separately.
3. Keep sensitive layers higher precision when mixed-layer tests justify it.
4. Select group size and kernel together; lower bits with slow dequant can lose.
5. Treat extreme 2-bit variants as distinct models with full quality gates.
6. Quantize KV independently and validate long-context tasks.
7. Record effective bits including scales/metadata.
8. Never quantize a generic model in foreground onboarding.
9. Bind a format to a proven kernel/device pair: NVIDIA FP8/FP4/INT4 features,
   AMD FP8/INT4/MFMA/WMMA paths, and Apple block-scaled/low-bit paths vary by
   accelerator generation and engine build.
10. Do not infer cross-vendor quality or speed from a shared format name; record
    the converter, packing layout, accumulation precision, and fallback kernel.

Artifacts include immutable digest, source/revision/license, converter commit,
format/topology, tokenizer/template digests, backend/version compatibility,
memory metadata, eval revision, and known-good recipes.

---

## KV and prefix-cache policy

Cache hierarchy:

1. active request KV;
2. leased session KV;
3. stable identity/policy/tool/workshop prefix blocks;
4. compiled grammar/tokenizer structures;
5. no unbudgeted global hidden cache.

Keys include artifact, backend/recipe, adapter, tokenizer/template, system/
identity/policy, tool schema, workshop/context, and relevant sampling semantics.

Soft-pressure eviction order is expired prefixes, unrelated sessions, draft
cache, old current-session history, optional draft model, then idle target. On
critical pressure the system cancels and terminates the worker.

For tool interruptions: retain KV for short local tools; time-bound retention for
network tools; release and recompute for human approval/long jobs; discard on
pressure regardless of expected duration.

---

## Decode acceleration order

This order optimizes an autoregressive lane; it is not the only generation
family MIR can select.

1. Correct safe recipe and pre-quantized weights.
2. Stable-prefix reuse.
3. Chunked prefill for responsiveness/cancellation.
4. Compiled/fused sampling and grammar masks.
5. N-gram/prompt-lookup speculation, especially code.
6. Draft model only if acceptance repays memory/load/energy.
7. DSpark-style semi-autoregressive drafting, with confidence scheduling tested
   separately against fixed/adaptive block lengths at batch one and burst load.
8. Speculative cascades combining lane deferral with target verification.
9. LayerSkip/Medusa/MTP/EAGLE-style checkpoint-coupled work after a stable model
   and training pipeline exist.
10. Custom kernels through the profiling gate.

Every layer is recipe-disableable. Metal, CUDA, HIP, quantization, and batch-one
behavior can reverse paper/datacenter results.

### DSpark integration gate

DSpark is a checkpoint-coupled technique, not a universal runtime toggle. Its
semi-autoregressive drafter, sequential head, confidence calibration, target
model, tokenizer/template, thinking mode, and maximum proposal length form one
immutable artifact/recipe identity. The worker contract therefore exposes
proposal confidences and variable verification lengths without making DSpark a
provider-level concept.

The first MIR experiment uses a released DeepSpec Qwen3 or Gemma target/drafter
pair and reproduces DeepSpec acceptance results before any native integration.
It then compares:

- target-only decoding;
- n-gram speculation;
- a conventional autoregressive draft model;
- DSpark with fixed verification length; and
- DSpark with calibrated confidence scheduling.

Tests run at concurrency one, foreground-plus-reflex bursts, and bounded desktop
queues. Metrics include accepted tokens per round, verification waste, draft and
target latency, confidence calibration, TTFT, token latency, combined resident
and peak memory, energy, load/unload time, and cancellation. CUDA results do not
promote Metal or HIP recipes; each backend needs equivalent kernels, semantics,
and a measured throughput curve. If confidence scheduling does not beat a
simpler adaptive length for Medousa's load shape, keep the drafter architecture
and discard the production-server scheduler.

### Diffusion and revisable generation

DiffusionGemma demonstrates a materially different local-inference shape: cache
the prompt, iteratively denoise a bidirectional token canvas, commit the finished
span, and continue block-autoregressively. MIR models that as generation events,
not fake autoregressive deltas:

```text
CanvasOpened(id, range, revision)
CanvasPatch(id, positions, provisional_tokens, confidence)
CanvasCommitted(id, text, token_range)
CanvasDiscarded(id, reason)
```

Only committed spans enter conversation history, tool parsing, prefix caches, or
external consumers. Home may show a provisional canvas only through an explicit
revisable-preview UI; the normal stream remains stable. Tool calls and structured
output require commit-time grammar/schema validation and deterministic failure
behavior.

The first experiment compares DiffusionGemma with quality- and footprint-aware
autoregressive Gemma on batch-one code infill, document editing, structured
blocks, chat, and tool calls. Measure preview latency, first-commit latency,
revisions, completion time, quality, resident/peak memory, energy, cancellation,
and load/unload—not only raw generated tokens per second. CUDA, ROCm, and Metal
are independent results; dedicated-GPU arithmetic-intensity gains are not
assumed on Apple unified memory.

---

## Interactive scheduler

- One foreground generation initially.
- Reflex work may run before heavyweight activation or on a separately admitted
  tiny worker only if coexistence wins measurements.
- User cancellation has highest priority.
- Chunk prefill so health/cancel checks happen between chunks.
- Background indexing/summarization yields to chat.
- Every request declares input/output/deadline/lane limits.
- Coordinator enforces output limits even if a backend does not.

Measure queue, admission, cold start, load, prefill, TTFT, token latency, tool
wait, cancellation acknowledgement, and stop separately.

---

## Control protocol and status

Use a private versioned control channel (Unix socket on macOS/Linux, suitable
named pipe on Windows) plus normalized generation stream. Loopback HTTP may be a
compatibility adapter but is not lifecycle truth.

Handshake: protocol version, worker generation, binary digest, backend/version,
features, artifact digest, recipe revision, phase, PID, and start time.

Replace `loaded: bool` with shared schema types:

```rust
enum LocalRuntimePhase {
    Unavailable, NotInstalled, Cold, Downloading, Admitting,
    StartingWorker, Loading, Warming, Ready, Busy, Draining,
    Unloading, Failed,
}

struct LocalRuntimeStatus {
    phase: LocalRuntimePhase,
    artifact: Option<ArtifactIdentity>,
    recipe: Option<RecipeIdentity>,
    backend: Option<BackendIdentity>,
    active_request: Option<RequestId>,
    memory: Option<RuntimeMemorySnapshot>,
    cache: Option<RuntimeCacheSnapshot>,
    since: Timestamp,
    message: String,
    recoverable: bool,
}
```

Normalized events cover admission/downgrade/rejection, worker start, load/warm,
response start, stable deltas, provisional canvas revisions and committed spans,
validated tool calls, metrics, cancellation, failure, and release/unload. Types
live in `medousa-types` and generate TS/SDK contracts.

---

## Observability and privacy

Record locally: identities, activation counts, RSS/physical footprint, available
host memory/pressure/swap, backend/device allocations, predicted/observed peak,
cache/KV/workspace estimates, load/unload/reclaimed memory, token counts,
prefill/decode, TTFT and token latency, speculation acceptance, cache reuse,
cancellation, exit, recovery, and available energy/thermal signals.

Platform collectors normalize but retain their source: Metal allocated bytes and
recommended working set; CUDA NVML VRAM/process use, utilization, power,
temperature, clocks, and throttling; AMD SMI VRAM/GTT/process use, utilization,
power, temperature, clocks, and RAS state; and Vulkan/DirectML/OS memory budgets
where vendor-native telemetry is unavailable. Missing counters are explicit,
never silently zero.

Never include prompts, model output, files, tool results, or credentials in
performance telemetry. Content capture is separate explicit developer mode with
synthetic/scrubbed input by default.

Settings eventually shows on-disk/warming/in-use/sleeping, disk vs memory,
selected recipe and rationale, safe context, last peak/recovery, unload, a
content-free diagnosis bundle, and reversible advanced overrides.

---

## Benchmark and evaluation laboratory

Every run manifest stores git state, engine commit/build flags, artifact/
tokenizer/template digests, every recipe argument, OS/machine/power state,
thermal/memory baseline, corpus revision, sampling seed, raw timings/memory, and
quality outputs or deterministic scores.

Performance suite:

- cold and warm requests;
- 2K/4K/8K/16K/32K prefill;
- short/long decode;
- tool and conversation prefix reuse;
- cancellation during download/load/prefill/decode;
- tool wait/resume;
- pressure while ready/busy;
- sleep/wake;
- 100-cycle load/unload soak; and
- crash/restart with stale state.

Metrics include TTFT, prefill/decode speed, p50/p95/p99 token latency, peak/
steady footprint, swap/compression, load/unload, reclaimed bytes at 1/5/10s,
utilization/energy, and UI responsiveness.

Quality has three layers: pinned public tasks (LM Eval, IFEval, BFCL, long
context), a versioned Medousa product suite, and small deterministic regression
probes. Comparisons control templates, prompts, sampling, context, and output.

Hardware coverage begins with a deliberate cross-platform matrix:

| Lane | Initial physical tiers |
|---|---|
| Apple Metal | M1 8 GB; M1/M2 16 GB; Pro/Max 16–32 GB; recent M-series 24–64 GB |
| NVIDIA CUDA on Linux | Consumer GPUs at approximately 8 GB, 12–16 GB, and 24 GB VRAM across at least two compute-capability generations |
| NVIDIA CUDA on Windows | At least one matching consumer GPU from the Linux matrix, tested natively under WDDM |
| AMD ROCm/HIP on Linux | Supported Radeon at approximately 8–12 GB and 16–24 GB plus one qualified Instinct-class reference when accessible; at least two `gfx` targets |
| AMD on Windows | One GPU/release pair explicitly listed for native HIP plus the same GPU's Vulkan fallback |
| CPU | x86-64 AVX2 baseline, a newer x86 vector/matrix tier, and Arm64 |

Every result names the exact GPU, device architecture, driver, runtime, OS build,
memory topology, power state, and engine package. Missing physical hardware is a
release gap for that recipe, not permission to infer from another vendor or OS.

Initial safety gates (calibrated by MIR-0):

- zero heavyweight worker/model RSS while cold;
- no worker before a real request;
- calibrated peak prediction within 15%;
- no new swap for healthy recommended short-context recipes;
- bounded termination on critical pressure;
- worker exit within 10 seconds of idle unload;
- at least 95% model footprint reclaimed or classified leaked;
- 100 lifecycle cycles with no upward footprint trend;
- one heavyweight worker maximum; and
- app/daemon survive worker OOM, crash, and kill.

TTFT/token-speed gates are per model and machine, never invented globally.

---

## Security and supply chain

- Content-address and sign model/engine packages.
- Pin catalog revision/license; forbid remote model code.
- Keep worker private; never expose it to LAN.
- Authenticate control generation.
- Limit worker files to model/runtime/request scope.
- Keep tool execution in Medousa's policy boundary.
- Exclude content/credentials from logs and diagnoses.
- Run conversion in developer/release pipelines, not silently for users.
- Treat adapters and tokenizer/templates with weight-level provenance.

---

## Delivery plan

### MIR-0 — Forensics and benchmark spine

Add a reproducible benchmark command and phase-by-phase memory sampling. Sweep
current artifacts across context/batch caps, compare ISQ to UQFF, reproduce idle
residency and lifecycle soak, and preserve current mistral.rs `0.8.1` as the
baseline.

**Exit:** ranked root causes and reproducible risk threshold without another
kernel panic.

### MIR-1 — Cold lifecycle and worker protocol

Add shared states, remove private brain from daemon startup, stop loading after
download/onboarding, activate on first request, add generation-aware handshake/
cancel/unload/shutdown, and evict on idle/sleep/switch/pressure.

**Exit:** zero worker cold, single-flight activation, reliable unload, and state
transition integration tests.

### MIR-2 — Cross-platform Resource Governor

Implement estimates, leases, macOS/Metal, CUDA/NVML, ROCm/AMD-SMI, Windows GPU-
budget, and host-memory probes; add pressure response, explicit context/output/
concurrency limits, learned observed peaks, and smaller/remote fallbacks with
visible rationale.

**Exit:** recommended recipes stay inside calibrated host and device envelopes
on the initial Metal, CUDA, and HIP machines.

### MIR-3 — Backend laboratory and resolver

Package pinned, optional llama.cpp CPU/Metal/CUDA/HIP/Vulkan backends first.
Upgrade mistral.rs in isolation; prototype MLX Swift/C++; reproduce BaseRT/Uzu;
test one stateful Core ML small model; benchmark TensorRT-LLM on NVIDIA Linux;
benchmark qualified vLLM/SGLang CUDA and ROCm lanes; and test native CUDA plus
HIP/Vulkan fallbacks on Windows. Normalize capabilities/metrics and produce
per-OS/device Pareto reports.

**Exit:** automatic selection across Metal, CUDA, and ROCm/HIP device classes,
with a stable CPU or Vulkan fallback and no platform-specific semantic drift.

### MIR-4 — Context compiler, cache, scheduler

Add category token accounting, relevant tools/compact schemas, prefix/session
budgets, chunked prefill/cancellation, tool-wait policy, and pressure eviction.

**Exit:** measured TTFT/prefill gain without cache correctness or memory failure.

### MIR-5 — Small-model capability router

Define reflex/worker/deep contracts, build product evals, benchmark current
0.5–5B candidates independent of brand, add validation-driven escalation and
grammar-constrained tools, and choose on quality/latency/memory/energy together.

**Exit:** small lanes beat a single larger baseline on product-weighted cost
while meeting quality floors.

### MIR-6 — Decode acceleration

Tune baseline; test n-grams and conventional draft pairs; reproduce one DeepSpec
DSpark checkpoint; isolate semi-autoregressive drafting from confidence
scheduling; measure acceptance/memory/energy across Metal, CUDA, and HIP; then
prototype self-speculation or other trained heads only for a stable winner. In a
separate alternative-generation track, reproduce DiffusionGemma against its
autoregressive family baseline and validate provisional-canvas/commit semantics.

**Exit:** every enabled technique wins end to end inside a declared envelope.

### MIR-7 — Adapter/distillation laboratory

Define adapter provenance, test task packs, build verified curricula, evaluate
SFT/on-policy distillation/repair data, and enforce privacy/license/rollback.

**Exit:** specialization improves target tasks without general or resource
regression.

### MIR-8 — Mobile/NPU lane

Compare Core ML and ExecuTorch, preserve coordinator/recipe semantics, measure
energy/thermal/background behavior, and keep remote workshop authority.

**Exit:** on-device ships only for explicit winning device/capability classes.

### MIR-9 — Native kernel investment

Select hotspots through the kernel gate, create shared correctness plus Metal,
CUDA, and HIP precision/device harnesses, ship behind recipes, upstream when
practical, and keep a portable fallback.

**Exit:** sustained material product gain; otherwise archive the negative result.

---

## First implementation sequence

The next branch does MIR-0 plus the MIR-1 skeleton only:

1. Add host-memory probes and a reproducible current-runtime benchmark; define
   the normalized Metal/CUDA/HIP telemetry schema without loading GPU runtimes.
2. Add lifecycle/status types without changing provider behavior.
3. Add explicit unload backed by process termination.
4. Remove every remaining `private_brain` side effect from core startup.
5. Make onboarding confirmation persist/download only.
6. Add request-time activation at the `medousa-local` provider boundary.
7. Enforce conservative 4K context, batch/concurrency 1 until measured recipes.
8. Add idle/switch/sleep/pressure termination and soak tests.

Do not upgrade engines in the same change; preserve a clean lifecycle baseline.

## Epic definition of done

- Install, selection, and residency are independent states.
- Cold truly means no worker/model memory.
- Activation is demand-driven and resource-leased.
- Every allocation belongs to an explainable recipe/budget.
- Every machine has a safe fallback.
- Backend selection is replaceable and measured.
- Metal, CUDA, and ROCm/HIP are first-class, independently tested lanes on their
  native operating systems; CPU and qualified portable GPU paths remain safe
  fallbacks.
- Vendor execution packages extend one worker/control protocol and never become
  core-daemon startup dependencies.
- Small models receive routing/context/constraints/retrieval/evaluation designed
  for them rather than being treated as weak generic chat models.
- Cancellation, sleep, switching, pressure, crash, and unload are tested product
  behavior.
- Benchmarks are reproducible and quality-gated.
- Research/resource decisions remain current in the ledger.
- New engines, quantizations, models, adapters, and kernels extend the system
  instead of reinventing it.
