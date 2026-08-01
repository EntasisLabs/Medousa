# Medousa inference research index

**Status:** living research ledger

**Last reviewed:** 2026-07-31

**Companion epic:** [medousa-inference-runtime-epic.md](medousa-inference-runtime-epic.md)

This index tracks the evidence behind Medousa's local inference decisions. It is
not a list of exciting techniques to implement indiscriminately. Every item is
connected to a product hypothesis, an experiment, or a decision gate.

## Evidence policy

We use three evidence classes:

1. **Upstream fact** — behavior documented by an engine or platform owner.
2. **Research result** — a paper result that must still be reproduced on
   Medousa workloads and consumer hardware.
3. **Medousa measurement** — a reproducible result from our benchmark harness.

Only class 3 can choose a production default. Paper speedups are orientation,
not promises. Each benchmark record must include the model artifact digest,
engine commit, runtime recipe, OS/build, hardware, prompt corpus revision, and
sampling configuration.

## Engine and platform sources

| Source | Evidence | What it contributes | Medousa question |
|---|---|---|---|
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) | Upstream | Rust-native serving, Metal/CUDA, UQFF/GGUF and other quantizations, continuous batching, PagedAttention, prefix caching, speculative decoding, runtime model load/unload, and hardware tuning | Can a current mistral.rs build beat other Mac engines without the memory behavior of our pinned `0.8.1` wrapper? |
| [mistral.rs releases](https://github.com/EricLBuehler/mistral.rs/releases) | Upstream | Tracks recent Metal memory fixes, Metal PagedAttention, AFQ/MXFP4, automatic quant selection, prefix cache changes, and speculative decoding work | Which improvements are available after upgrading, and which are stable on every supported Mac generation? |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | Upstream | Mature GGUF runtime; first-class Apple Silicon/Metal; broad CPU/GPU support; low-bit formats; hybrid offload | Is GGUF + llama.cpp the safest cross-platform baseline and/or the best Apple decode engine? |
| [llama-server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md) | Upstream | Explicit context, batch, micro-batch, parallelism, Flash Attention, continuous batching, metrics, structured output, and speculative controls | Which controls must appear in Medousa's backend-neutral recipe contract? |
| [llama.cpp speculative decoding](https://github.com/ggml-org/llama.cpp/blob/master/docs/speculative.md) | Upstream | Draft-model and n-gram speculative decoding | Does n-gram speculation improve code/editing workloads without another resident model? When does a draft model repay its memory? |
| [MLX](https://github.com/ml-explore/mlx) | Upstream | Apple-owned array framework with unified memory, lazy execution, C/C++/Swift APIs, and graph transformations | Should an Apple-specific backend be first-class rather than forcing all Macs through a cross-platform engine? |
| [MLX lazy evaluation](https://ml-explore.github.io/mlx/build/html/usage/lazy_evaluation.html) | Upstream | Deferred materialization can avoid unnecessary peak allocations when loading lower-precision weights | Can MLX conversion/loading eliminate the full-precision-plus-quantized peak seen in in-situ quantization? |
| [MLX compilation](https://ml-explore.github.io/mlx/build/html/usage/compile.html) | Upstream | Graph fusion/compilation can reduce runtime and memory use | Which fixed-shape decode paths benefit enough to precompile during a bounded warm-up? |
| [MLX unified memory](https://ml-explore.github.io/mlx/build/html/usage/unified_memory.html) | Upstream | CPU and GPU directly access one memory pool | Memory accounting must cover the whole process footprint; “GPU memory” is not an independent budget on Apple Silicon. |
| [MLX LM](https://github.com/ml-explore/mlx-lm) | Upstream | Quantized model conversion, rotating KV cache, prompt caching, batch generation, and Apple-specific model support | Can its cache controls and model conversions establish the Mac performance ceiling, even if Python is not shipped? |
| [MLX Swift LM](https://github.com/ml-explore/mlx-swift-lm) | Upstream | Native Swift LLM/VLM loading and generation on MLX | Could a Swift worker avoid a Python runtime while retaining MLX performance? |
| [BaseRT](https://github.com/basecompute/baseRT) and [paper](https://arxiv.org/abs/2607.00501) | Upstream + research | New native-Metal runtime focused on single-user edge inference; its paper reports particularly strong decode gains on small models. Its CLI/format/bindings are Apache-2.0, but the shipped engine binary is proprietary. | Add to the Mac benchmark lab, but treat it as an optional proprietary integration unless the engine itself becomes suitably open. Reproduce memory, unload, quality, and packaging claims. |
| [Uzu](https://github.com/trymirai/uzu) | Upstream | MIT-licensed Rust engine for Apple unified memory, with Rust/Swift bindings and structured output; BaseRT's study reports competitive prefill behavior from its GPU/MPSGraph approach | Does its open implementation improve prefill and energy enough to become a backend or teach MIR's own Metal path? |
| [Metal recommended working set](https://developer.apple.com/documentation/metal/mtldevice/recommendedmaxworkingsetsize) | Upstream | Device-specific working-set guidance and allocated-size reporting | The Mac admission controller should use Metal limits in addition to total/available system RAM. |
| [Metal feature limits](https://developer.apple.com/metal/limits/) | Upstream | Generation-specific tensor formats and capabilities, including newer low-bit/block-scaled paths | Recipes must be chip-family aware rather than merely checking `aarch64 + Metal`. |
| [Core ML stateful models](https://apple.github.io/coremltools/docs-guides/source/stateful-models.html) | Upstream | Stateful KV cache on macOS 15/iOS 18; Apple demonstrates substantially faster autoregressive prediction than copying state through model I/O | Is Core ML competitive for a curated, fixed small-model lane and mobile deployment? |
| [Core ML optimization overview](https://apple.github.io/coremltools/docs-guides/source/opt-overview.html) | Upstream | INT4/INT8, palettization, pruning, and hardware-specific guidance; explicitly recommends device/model measurement | Which Neural Engine/GPU recipe wins on M-series generations and supported OS versions? |
| [ExecuTorch backend matrix](https://docs.pytorch.org/executorch/stable/pathway-quickstart.html) | Upstream | Core ML/MPS/XNNPACK on Apple, QNN/MediaTek on Android, OpenVINO on desktop, selective builds | Is ExecuTorch the eventual mobile/NPU packaging lane while desktop remains process-backed? |
| [ExecuTorch LLM export](https://docs.pytorch.org/executorch/stable/llm/export-llm.html) | Upstream | KV-cache export, backend lowering, quantization, and delegate inspection | What conversion/evaluation work is required for a curated mobile artifact? |
| [Candle](https://github.com/huggingface/candle) | Upstream | Low-level Rust tensor/model framework and Metal kernels | Keep as an implementation substrate and experimentation option; do not confuse it with a complete product runtime. |
| [MLC LLM engine configuration](https://llm.mlc.ai/docs/deploy/rest.html) | Upstream | Explicit sequence, total token, memory utilization, chunked prefill, sliding window, prefix cache, and speculative controls across Metal/Vulkan/CUDA | Use as a reference for the minimum completeness of Medousa's backend-neutral engine recipe. |

## Serving and cache systems

| Source | Evidence | Result or mechanism | Applicability to Medousa |
|---|---|---|---|
| [PagedAttention / vLLM](https://arxiv.org/abs/2309.06180) | Research | Page-like KV blocks reduce fragmentation and permit cache sharing | Valuable semantics, but our initial single-user runtime should not absorb datacenter scheduler complexity without measured benefit. |
| [SGLang / RadixAttention](https://arxiv.org/abs/2312.07104) | Research | Radix-tree prefix reuse and compressed FSMs accelerate structured multi-call programs | Highly relevant: Medousa agents repeatedly share identity, tool, workshop, and conversation prefixes. |
| [SARATHI](https://arxiv.org/abs/2308.16369) and [Sarathi-Serve](https://arxiv.org/abs/2403.02310) | Research | Chunked prefill avoids long prefill stalls and can interleave decode work | Adapt for UI responsiveness and cancellation, not high batch throughput. A long vault context must not freeze an active chat stream. |
| [INFERCEPT](https://arxiv.org/abs/2402.01869) | Research | Cache policies for interrupted tool-augmented inference | Directly relevant to agents that pause for tools: retain, compress, or discard session KV based on expected tool latency and memory pressure. |
| [XGrammar](https://arxiv.org/abs/2411.15100) | Research | Near-zero-overhead grammar masks through precomputation and overlap | Structured tool calls can make a small model more reliable without expensive retry loops. |
| [XGrammar 2](https://arxiv.org/abs/2601.04426) | Research | Dynamic dispatch, JIT grammar compilation, and cross-grammar caching for agentic output | Track for dynamic tools and environment-derived constraints; do not implement before basic schema-constrained decoding is measured. |

## Weight and KV quantization

| Source | Evidence | Reported result | Decision posture |
|---|---|---|---|
| [AWQ](https://arxiv.org/abs/2306.00978) | Research | Activation-aware protection of salient weights for low-bit weight-only inference | Candidate format when supported by a fast device kernel; quality alone is insufficient. |
| [GPTQ](https://arxiv.org/abs/2210.17323) | Research | Post-training weight quantization using approximate second-order information | Baseline quality comparison; prefer formats with mature Metal kernels. |
| [AQLM](https://arxiv.org/abs/2401.06118) | Research | Additive codebooks improve extreme 2-bit quality/size tradeoffs | Research lane only until Apple kernels outperform a smaller 3–4-bit model. |
| [KIVI](https://arxiv.org/abs/2402.02750) | Research | Asymmetric 2-bit KV quantization; paper reports lower peak memory and higher serving throughput | Begin with engine-supported 8/4-bit KV; evaluate 2-bit only against long-context Medousa quality suites. |
| [KVQuant](https://arxiv.org/abs/2401.18079) | Research | Per-channel keys, pre-RoPE quantization, non-uniform values, and sparse outlier handling | Informs a future custom KV format; not a first milestone. |
| [KVLinC](https://arxiv.org/abs/2510.05373) | Research | Rotation and linear correction for very-low-bit KV cache | Watchlist for long-context specialist recipes. |
| [TurboQuant](https://research.google/blog/turboquant-redefining-ai-efficiency-with-extreme-compression/) | Research-owner summary | Rotation plus low-overhead residual quantization targets KV/vector compression; Google reports a 3-bit KV result across long-context suites | High-priority watchlist for both inference KV and Medousa vector indexes. Requires consumer-Metal kernels and independent task reproduction. |
| [QQQ](https://arxiv.org/abs/2406.09904) | Research | W4A8 targets both compute-bound prefill and bandwidth-bound decode | Useful distinction: weight-only compression may accelerate decode but not necessarily prefill. Benchmark both phases separately. |
| [Meta quantized Llama](https://ai.meta.com/blog/meta-llama-quantized-lightweight-models/) | Upstream + measured release | QAT and SpinQuant variants reduced model size/memory and improved mobile speed in Meta's tests | Prefer publisher-produced QAT artifacts when license, engine, and task quality fit; never assume arbitrary post-training Q4 is equivalent. |

## Decode acceleration

| Source | Evidence | Mechanism | Medousa order of operations |
|---|---|---|---|
| [Speculative decoding in llama.cpp](https://github.com/ggml-org/llama.cpp/blob/master/docs/speculative.md) | Upstream | Draft model or n-gram candidates are verified by the target model | Test n-gram first because it adds little resident memory and may suit code/repetition. |
| [Medusa](https://arxiv.org/abs/2401.10774) | Research | Multiple trained decode heads propose a tree of future tokens | Attractive for a Medousa-tuned model family, but requires checkpoint-specific training and engine support. |
| [EAGLE-3](https://arxiv.org/abs/2503.01840) | Research | A learned draft uses fused multi-layer target features; paper reports large single-request speedups and more modest batched throughput gains | Candidate after a stable target model and training/evaluation pipeline exist. Acceptance rate, memory, and Metal kernels decide viability. |
| [LayerSkip](https://arxiv.org/abs/2404.16710) | Research | Training with early-exit loss enables self-speculative decoding using one model | Particularly interesting under tight memory because it avoids a second full draft model, but requires compatible training. |
| [SpecInfer](https://arxiv.org/abs/2305.09781) | Research | Tree candidates from one or more speculative models are verified together | Datacenter-oriented; borrow verification concepts only if simpler speculation leaves performance on the table. |
| [Speculative cascades](https://research.google/blog/speculative-cascades-a-hybrid-approach-for-smarter-faster-llm-inference/) | Research-owner summary | Combines small-to-large deferral with speculative verification instead of treating routing and speculation as separate systems | Closely matches MIR's reflex/worker/deep lanes; evaluate after validation-driven escalation and a draft/target pair both exist. |

Speculation is not automatically faster. It trades additional compute, cache, and
complexity for fewer target decode steps. We gate it on:

- accepted tokens per target pass;
- end-to-end tokens/second, not draft speed;
- additional resident and peak memory;
- time to first token;
- exact sampling-distribution correctness where promised;
- tool-call and code quality; and
- per-chip results, especially Metal.

## Small-model capability

| Source | Evidence | Relevant lesson | Medousa use |
|---|---|---|---|
| [Gemma 4 overview](https://ai.google.dev/gemma/docs/core) and [technical report](https://arxiv.org/abs/2607.02770) | Upstream + research | Family spans edge-sized, unified, dense, and MoE variants with hardware-specific architecture choices | Keep Gemma in the candidate set, but stop making a single family an architectural dependency. |
| [Qwen3](https://qwenlm.github.io/blog/qwen3/) | Upstream | Dense open models from 0.6B through 32B and hybrid thinking modes | Strong candidate family for the reflex/worker lanes and draft-target pairing. |
| [SmolLM3](https://huggingface.co/blog/smollm3) | Upstream technical release | 3B model uses GQA to reduce KV cache, hybrid positional design, long context, dual reasoning mode, and a published training recipe | Useful open baseline for transparent small-model experiments and task-specific post-training. |
| [Phi-4 Mini report](https://arxiv.org/abs/2503.01743) | Research | High-quality/synthetic data and post-training make a 3.8B model competitive on math/code; multimodal adaptation uses a mixture of LoRAs | Candidate worker model and evidence that data/adaptation quality can matter more than raw parameter count. |
| [Minitron](https://arxiv.org/abs/2407.14679) | Research | Structured pruning plus distillation can derive compact models from a larger parent with far fewer retraining tokens | Long-term path for a Medousa model family; not required to ship the runtime. |
| [Small-model post-training via distillation](https://arxiv.org/abs/2509.26497) | Research | Curriculum SFT plus on-policy distillation improves billion-parameter models | Supports a task-curriculum approach rather than one generic chat fine-tune. |
| [Step-wise on-policy distillation for agents](https://arxiv.org/abs/2605.07725) | Research | Tool-call errors compound; step-level divergence can gate teacher supervision | Directly relevant if Medousa trains a reflex/worker model from successful tool trajectories. |
| [Self-distilled reasoner](https://arxiv.org/abs/2601.18734) | Research | Privileged verified traces can supervise a weaker policy on its own rollouts | Research lane for verified code/tool traces; requires strict privacy and contamination controls. |

The product hypothesis is not “a 2B model can answer everything.” It is:

> A small, fast model can feel much larger when the system gives it the right
> bounded task, compact tools, reusable prefix state, constrained output,
> retrieval, execution feedback, and escalation path.

## Evaluation sources

| Source | What it measures | Use in Medousa |
|---|---|---|
| [LM Evaluation Harness](https://github.com/EleutherAI/lm-evaluation-harness) | Reproducible academic and task evaluations | General capability and regression layer; pin task YAML and harness commit. |
| [IFEval](https://arxiv.org/abs/2311.07911) | Verifiable instruction following | Detect quantization, context, or post-training regressions without relying only on an LLM judge. |
| [Berkeley Function Calling Leaderboard](https://gorilla.cs.berkeley.edu/leaderboard) | Single-turn, multi-turn, live, hallucination, and agentic function calling | External tool-use baseline; supplement with Medousa's actual schemas and execution checks. |
| [LongBench v2](https://github.com/THUDM/LongBench) | Long-context understanding and reasoning | Evaluate context/KV recipes; do not advertise model maximum context as usable context. |

Public benchmarks are necessary but insufficient. Production selection is
weighted toward a versioned `medousa-eval` corpus:

- conversation and instruction following;
- coding edits and repository navigation;
- tool selection, argument correctness, and hallucinated tools;
- schema-constrained generation;
- document extraction and summarization;
- memory/retrieval grounding;
- cancellation and resume around tool waits;
- adversarial prompt and tool-result boundaries; and
- latency-sensitive “reflex” tasks such as classification and title generation.

## Research backlog

Each item begins as a question, not a commitment.

| ID | Question | Minimum experiment | Promotion gate |
|---|---|---|---|
| R-01 | Which engine is best for each Apple chip/model/format? | Identical model family and quality target across mistral.rs, llama.cpp, MLX, and eligible Core ML artifacts | Pareto frontier for quality, TTFT, decode, peak memory, unload recovery, and energy |
| R-02 | Does pre-quantized UQFF avoid the current load spike? | Compare UQFF load with in-situ quantization under process physical-footprint sampling | Lower peak without quality or steady-speed regression |
| R-03 | Is GGUF the best cross-platform distribution format? | Same target model at comparable effective bits on Metal, CPU, CUDA | Operational simplicity plus competitive Pareto result |
| R-04 | How small can the reflex model be? | 0.6B–2B candidates on Medousa routing/tool/schema tasks | Meets task accuracy and hallucination gates at materially lower latency/energy |
| R-05 | Does prefix caching materially improve Home? | Identity + tool schema + workshop prefix hit-rate replay | Significant TTFT/prefill reduction within a strict cache memory budget |
| R-06 | What is the safe default context? | 2K/4K/8K/16K/32K sweeps per tier and model | Quality benefit justifies KV, TTFT, and responsiveness cost |
| R-07 | Can n-gram speculation accelerate code? | Code edits, patches, and prose with/without prompt n-gram cache | Positive p50 and p95 end-to-end result with negligible memory cost |
| R-08 | When does a draft model pay off? | Candidate draft/target pairs across sampling temperatures and tasks | Speed gain after counting draft memory, load time, energy, and acceptance |
| R-09 | Can LayerSkip/Medusa/EAGLE justify training ownership? | One stable worker checkpoint and reproducible baseline | Large enough speedup to repay checkpoint coupling and maintenance |
| R-10 | Which KV precision is safe? | FP16/BF16/8/4/2-bit sweeps on long chat, code, retrieval, and tool tasks | Memory improvement without unacceptable task degradation |
| R-11 | Should tool waits retain KV? | Replay short/medium/long tool latency under memory pressure | Adaptive policy beats always-retain and always-discard |
| R-12 | Can adapter packs specialize one resident base? | Code/messaging/vault LoRA packs with load/switch measurements | Capability gain with bounded memory and no destructive interference |
| R-13 | Is an Apple Neural Engine lane worthwhile? | Curated 1–4B Core ML artifact versus Metal engines | Better energy/latency for supported tasks without unacceptable conversion lock-in |
| R-14 | Where are custom Metal kernels justified? | Profile top operations after engine/recipe optimization | Stable hotspot with no acceptable upstream implementation and a testable kernel contract |
| R-15 | Does a native-Metal runtime change our build-vs-integrate boundary? | Reproduce BaseRT and Uzu against MLX, llama.cpp, and mistral.rs on identical artifacts and Macs | Meaningful quality-equivalent product gain plus acceptable license, packaging, lifecycle, and maintainability; proprietary BaseRT results may guide but cannot silently become core infrastructure |
| R-16 | Can speculative cascades unify routing and decode acceleration? | Reflex/worker pair with calibrated deferral confidence and target verification | Better quality/latency frontier than plain routing and plain speculative decoding, including both resident footprints |
| R-17 | Can TurboQuant-class compression help local KV and retrieval together? | Reference implementation on long-context tasks and representative vector indexes | Lower combined memory with negligible task/recall loss and a fast consumer-hardware kernel |

## Maintenance cadence

- Re-run the engine matrix for major engine releases, new Apple GPU families,
  and every catalog model change.
- Review this source ledger monthly while the epic is active, then at least once
  per Medousa release train.
- Record negative results. A technique rejected on M1 may remain viable on M5,
  and a technique that wins throughput may still lose interactive latency.
- Never silently replace a model artifact. Catalog entries are immutable by
  digest; new quantizations receive new recipe revisions.
