# Medousa inference research index

**Status:** living research ledger

**Last reviewed:** 2026-08-01

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

## Platform coverage contract

Metal, CUDA, and ROCm/HIP are equal product lanes. A technique is not described
as a Medousa runtime capability until its supported OS, accelerator family,
driver/runtime range, model format, and fallback have been recorded. Linux-only
serving results do not establish Windows support, and Apple unified-memory
results do not predict discrete-VRAM behavior.

| User platform | Preferred native lane | Required baseline or fallback | Important constraint |
|---|---|---|---|
| macOS on Apple silicon | Metal through the winning measured backend; Core ML for curated models | CPU | System RAM, GPU allocations, and the OS share unified memory. |
| Linux with NVIDIA GPU | CUDA | CPU; Vulkan where it wins and is supported | Recipes are keyed by compute capability, driver, CUDA runtime, VRAM, and kernel availability. |
| Windows with NVIDIA GPU | Native CUDA | CPU or Vulkan; WSL is an explicit advanced lane, never an invisible requirement | Native packaging and lifecycle must be tested independently from Linux containers. |
| Linux with AMD GPU | ROCm/HIP | CPU or Vulkan | Recipes are keyed by `gfx` target, ROCm/kernel/driver compatibility, VRAM topology, and supported kernels. |
| Windows with AMD GPU | Native HIP only for a jointly supported ROCm/Windows release and GPU | Vulkan or CPU; DirectML remains a research fallback | Windows support trails or differs from Linux, so catalog compatibility must never be inferred from “ROCm supported.” |
| Any supported desktop without a qualified GPU lane | Optimized CPU | — | x86 instruction set and Arm64 capability remain part of the recipe identity. |

The first portability target is one immutable GGUF artifact evaluated through
llama.cpp on CPU, Metal, CUDA, HIP, and Vulkan. Vendor-specialized artifacts or
engines may replace that baseline only when their quality-equivalent product
measurements win on a declared device class.

## Cross-platform engine sources

| Source | Evidence | What it contributes | Medousa question |
|---|---|---|---|
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) | Upstream | Rust-native serving, Metal/CUDA, UQFF/GGUF and other quantizations, continuous batching, PagedAttention, prefix caching, speculative decoding, runtime model load/unload, and hardware tuning | On which Metal and CUDA device classes does it beat the portable baseline, and what is the AMD fallback? |
| [mistral.rs releases](https://github.com/EricLBuehler/mistral.rs/releases) | Upstream | Tracks memory fixes, PagedAttention, AFQ/MXFP4, automatic quant selection, prefix-cache changes, and speculative decoding work | Which improvements are stable on each supported OS, accelerator family, and driver range? |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | Upstream | Mature GGUF runtime; CPU, Metal, CUDA, HIP, Vulkan, SYCL, and other backends; low-bit formats; hybrid offload | Is GGUF + llama.cpp the safest common distribution/runtime baseline, and where does a specialized engine beat it? |
| [llama-server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md) | Upstream | Explicit context, batch, micro-batch, parallelism, Flash Attention, continuous batching, metrics, structured output, and speculative controls | Which controls must appear in Medousa's backend-neutral recipe contract? |
| [llama.cpp speculative decoding](https://github.com/ggml-org/llama.cpp/blob/master/docs/speculative.md) | Upstream | Draft-model and n-gram speculative decoding | Does n-gram speculation improve code/editing workloads without another resident model? When does a draft model repay its memory? |
| [MLC LLM engine configuration](https://llm.mlc.ai/docs/deploy/rest.html) | Upstream | Explicit sequence, total-token, memory-utilization, chunked-prefill, sliding-window, prefix-cache, and speculative controls across Metal/Vulkan/CUDA | Use as a reference for the minimum completeness of Medousa's backend-neutral engine recipe and as a portable candidate. |
| [ExecuTorch backend matrix](https://docs.pytorch.org/executorch/stable/pathway-quickstart.html) | Upstream | Core ML/MPS/XNNPACK on Apple, QNN/MediaTek on mobile, and OpenVINO on desktop | Is ExecuTorch the eventual mobile/NPU packaging lane while desktop remains process-backed? |

## NVIDIA CUDA sources

| Source | Evidence | What it contributes | Medousa question |
|---|---|---|---|
| [CUDA compatibility](https://docs.nvidia.com/deploy/cuda-compatibility/) | Upstream | Driver/toolkit backward, minor-version, and forward-compatibility rules; PTX and newer-feature caveats | Which CUDA runtime and SASS targets can Medousa package safely for both Windows and Linux without forcing a toolkit install? |
| [CUDA Toolkit release notes](https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/) | Upstream | Minimum driver versions and OS-specific availability for each toolkit release | The package resolver must reject or select a compatible build before starting a worker. |
| [NVML API](https://docs.nvidia.com/deploy/nvml-api/) | Upstream | Device/process memory, utilization, temperature, power, clocks, and throttle telemetry | Which counters reliably drive admission, pressure response, and content-free diagnosis on consumer GPUs? |
| [nvidia-smi selective queries](https://docs.nvidia.com/deploy/nvidia-smi/index.html#selective-query-options) | Upstream | Stable CSV queries over NVML-backed device and compute-process counters without linking CUDA into the control plane | Bootstrap the benchmark collector now; replace command parsing with a pinned NVML package when admission begins enforcing device leases. |
| [TensorRT-LLM](https://github.com/NVIDIA/TensorRT-LLM) | Upstream | NVIDIA-specialized kernels, quantization, paged KV cache, in-flight batching, and speculative decoding | Does it improve batch-one interactive latency enough to repay Linux-centric packaging, engine-plan construction, and checkpoint coupling? |
| [CUTLASS](https://github.com/NVIDIA/cutlass) | Upstream | Architecture-specific CUDA tensor-core abstractions and kernels across mixed precision, FP8, FP4/MX formats, and INT4 | Which maintained primitives cover a measured MIR hotspot, and which compute capabilities/build platforms actually support them? |
| [Nsight Compute](https://docs.nvidia.com/nsight-compute/) | Upstream | Kernel-level profiling, occupancy, memory, instruction, and roofline-style analysis | Establish evidence for a CUDA custom-kernel gate rather than porting a fashionable kernel blindly. |
| [vLLM GPU installation](https://docs.vllm.ai/en/latest/getting_started/installation/gpu/) | Upstream | CUDA and ROCm serving lanes with explicit OS/GPU requirements | Use in the Linux benchmark lab; do not assume its server-oriented scheduler or platform matrix is a desktop product fit. |
| [SGLang backend support](https://docs.sglang.ai/) | Upstream | CUDA/ROCm serving, RadixAttention, structured generation, and speculative techniques | Use as a research oracle for agent-prefix and decode techniques; qualify packaging and batch-one behavior separately. |

## AMD ROCm/HIP sources

| Source | Evidence | What it contributes | Medousa question |
|---|---|---|---|
| [ROCm compatibility matrix](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html) | Upstream | Supported operating systems, kernels, hardware, drivers, and ROCm releases | Which exact `gfx` targets and host combinations receive a native HIP recipe? |
| [ROCm on Windows release versioning](https://rocm.docs.amd.com/projects/install-on-windows/en/latest/about/release-versioning.html) | Upstream | Identifies joint Linux/Windows releases and Windows-specific version support | Prevent the catalog from promising Windows parity based on a Linux ROCm result. |
| [HIP programming model](https://rocm.docs.amd.com/projects/HIP/en/latest/) | Upstream | AMD's portable GPU execution API and CUDA-porting layer | Can kernels remain source-portable while recipes and compiled targets stay hardware-specific? |
| [AMD SMI](https://rocm.docs.amd.com/projects/amdsmi/en/latest/) | Upstream | Device/process VRAM, utilization, power, temperature, clock, and RAS telemetry | Provide AMD-native admission and diagnosis without parsing command output. |
| [AMD SMI CLI JSON](https://rocm.docs.amd.com/projects/amdsmi/en/latest/how-to/amdsmi-cli-tool.html) | Upstream | JSON-capable device, monitor, and process telemetry; AMD describes the CLI as an example and recommends the C++/Python library for robust collection | Use tolerant JSON only for the benchmark bootstrap, preserve missing fields, then move production admission to the packaged library. |
| [Composable Kernel](https://github.com/ROCm/composable_kernel) | Upstream | Performance-portable HIP kernels for tensor operations across AMD architectures | Which small-model decode hotspots already have maintained kernels before MIR writes one? |
| [AITER](https://github.com/ROCm/aiter) | Upstream | AMD's inference/training operator library using Triton, Composable Kernel, and tuned assembly | Which production operators apply to supported consumer Radeon targets rather than only Instinct? |
| [ROCprofiler-SDK](https://rocm.docs.amd.com/projects/rocprofiler-sdk/en/latest/) | Upstream | HIP/kernel/memory tracing and low-level counter collection | Establish evidence for the HIP custom-kernel gate and expose architecture-specific bottlenecks. |
| [AMD Quark](https://quark.docs.amd.com/) | Upstream | AMD quantization tooling and hardware-aware optimization | Which quantized artifacts and kernels are actually supported on consumer Radeon versus Instinct? |
| [vLLM GPU installation](https://docs.vllm.ai/en/latest/getting_started/installation/gpu/) | Upstream | A maintained ROCm serving implementation with a published support matrix | Linux performance-ceiling candidate only after consumer-GPU support, unload, and batch-one behavior are reproduced. |

## Apple Metal sources

| Source | Evidence | What it contributes | Medousa question |
|---|---|---|---|
| [MLX](https://github.com/ml-explore/mlx) | Upstream | Apple-owned array framework with unified memory, lazy execution, C/C++/Swift APIs, and graph transformations | Should an Apple-specific backend be first-class rather than forcing all Macs through a cross-platform engine? |
| [MLX lazy evaluation](https://ml-explore.github.io/mlx/build/html/usage/lazy_evaluation.html) | Upstream | Deferred materialization can avoid unnecessary peak allocations when loading lower-precision weights | Can MLX conversion/loading eliminate the full-precision-plus-quantized peak seen in in-situ quantization? |
| [MLX compilation](https://ml-explore.github.io/mlx/build/html/usage/compile.html) | Upstream | Graph fusion/compilation can reduce runtime and memory use | Which fixed-shape decode paths benefit enough to precompile during a bounded warm-up? |
| [MLX unified memory](https://ml-explore.github.io/mlx/build/html/usage/unified_memory.html) | Upstream | CPU and GPU directly access one memory pool | Memory accounting must cover the whole process footprint; “GPU memory” is not an independent budget on Apple Silicon. |
| [MLX LM](https://github.com/ml-explore/mlx-lm) | Upstream | Quantized model conversion, rotating KV cache, prompt caching, batch generation, and Apple-specific model support | Can its cache controls and model conversions establish the Mac performance ceiling, even if Python is not shipped? |
| [MLX Swift LM](https://github.com/ml-explore/mlx-swift-lm) | Upstream | Native Swift LLM/VLM loading and generation on MLX | Could a Swift worker avoid a Python runtime while retaining MLX performance? |
| [BaseRT](https://github.com/basecompute/baseRT) and [paper](https://arxiv.org/abs/2607.00501) | Upstream + research | New native-Metal runtime focused on single-user edge inference; its paper reports particularly strong decode gains on small models. Its CLI/format/bindings are Apache-2.0, but the shipped engine binary is proprietary. | Add to the Mac benchmark lab, but treat it as an optional proprietary integration unless the engine itself becomes suitably open. Reproduce memory, unload, quality, and packaging claims. |
| [Uzu](https://github.com/trymirai/uzu) | Upstream | MIT-licensed Rust engine for Apple unified memory, with Rust/Swift bindings and structured output; BaseRT's study reports competitive prefill behavior from its GPU/MPSGraph approach | Does its open implementation improve prefill and energy enough to become a backend or teach MIR's own Metal path? |
| [Metal recommended working set](https://developer.apple.com/documentation/metal/mtldevice/recommendedmaxworkingsetsize) | Upstream | Device-specific working-set guidance and allocated-size reporting | The Mac admission controller should use Metal limits in addition to total/available system RAM. |
| [Metal current allocated size](https://developer.apple.com/documentation/metal/mtldevice/currentallocatedsize) | Upstream | Process allocations for resources created through a Metal device | Sample the benchmark process at every phase and compare the observed peak with host RSS before enforcing the working-set envelope. |
| [Metal feature limits](https://developer.apple.com/metal/limits/) | Upstream | Generation-specific tensor formats and capabilities, including newer low-bit/block-scaled paths | Recipes must be chip-family aware rather than merely checking `aarch64 + Metal`. |
| [Core ML stateful models](https://apple.github.io/coremltools/docs-guides/source/stateful-models.html) | Upstream | Stateful KV cache on macOS 15/iOS 18; Apple demonstrates substantially faster autoregressive prediction than copying state through model I/O | Is Core ML competitive for a curated, fixed small-model lane and mobile deployment? |
| [Core ML optimization overview](https://apple.github.io/coremltools/docs-guides/source/opt-overview.html) | Upstream | INT4/INT8, palettization, pruning, and hardware-specific guidance; explicitly recommends device/model measurement | Which Neural Engine/GPU recipe wins on M-series generations and supported OS versions? |
| [ExecuTorch LLM export](https://docs.pytorch.org/executorch/stable/llm/export-llm.html) | Upstream | KV-cache export, backend lowering, quantization, and delegate inspection | What conversion/evaluation work is required for a curated mobile artifact? |
| [Candle](https://github.com/huggingface/candle) | Upstream | Low-level Rust tensor/model framework and Metal kernels | Keep as an implementation substrate and experimentation option; do not confuse it with a complete product runtime. |

## Portable fallback sources

| Source | Evidence | What it contributes | Medousa question |
|---|---|---|---|
| [llama.cpp build matrix](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md) | Upstream | Separately buildable CPU, Metal, CUDA, HIP, Vulkan, SYCL, and OpenVINO backends; runtime device selection and dynamic backend loading | Can vendor backend packages share one worker protocol and GGUF catalog while remaining optional downloads? |
| [Vulkan](https://docs.vulkan.org/spec/latest/) | Upstream | Cross-vendor GPU compute and explicit memory-budget primitives | Is Vulkan the dependable Windows AMD and unsupported-device fallback without making it the lowest common denominator? |
| [ONNX Runtime execution providers](https://onnxruntime.ai/docs/execution-providers/) | Upstream | CUDA, TensorRT, MIGraphX, DirectML, Core ML, OpenVINO, and CPU provider model; its older ROCm provider is deprecated | Useful portability reference and possible curated-model lane, but not evidence for a current generic ROCm path; LLM feature completeness must be measured. |
| [OpenVINO](https://docs.openvino.ai/) | Upstream | Intel CPU/GPU/NPU inference and model optimization | Preserve a future Intel accelerator lane while CPU remains universally safe. |

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
| [GPTQ](https://arxiv.org/abs/2210.17323) | Research | Post-training weight quantization using approximate second-order information | Baseline quality comparison; prefer formats with mature kernels on the target Metal, CUDA, or HIP device. |
| [AQLM](https://arxiv.org/abs/2401.06118) | Research | Additive codebooks improve extreme 2-bit quality/size tradeoffs | Research lane only until target-device kernels outperform a smaller 3–4-bit model. |
| [KIVI](https://arxiv.org/abs/2402.02750) | Research | Asymmetric 2-bit KV quantization; paper reports lower peak memory and higher serving throughput | Begin with engine-supported 8/4-bit KV; evaluate 2-bit only against long-context Medousa quality suites. |
| [KVQuant](https://arxiv.org/abs/2401.18079) | Research | Per-channel keys, pre-RoPE quantization, non-uniform values, and sparse outlier handling | Informs a future custom KV format; not a first milestone. |
| [KVLinC](https://arxiv.org/abs/2510.05373) | Research | Rotation and linear correction for very-low-bit KV cache | Watchlist for long-context specialist recipes. |
| [TurboQuant](https://research.google/blog/turboquant-redefining-ai-efficiency-with-extreme-compression/) | Research-owner summary | Rotation plus low-overhead residual quantization targets KV/vector compression; Google reports a 3-bit KV result across long-context suites | High-priority watchlist for both inference KV and Medousa vector indexes. Requires fast kernels and independent reproduction on consumer Metal, CUDA, and HIP targets. |
| [QQQ](https://arxiv.org/abs/2406.09904) | Research | W4A8 targets both compute-bound prefill and bandwidth-bound decode | Useful distinction: weight-only compression may accelerate decode but not necessarily prefill. Benchmark both phases separately. |
| [Meta quantized Llama](https://ai.meta.com/blog/meta-llama-quantized-lightweight-models/) | Upstream + measured release | QAT and SpinQuant variants reduced model size/memory and improved mobile speed in Meta's tests | Prefer publisher-produced QAT artifacts when license, engine, and task quality fit; never assume arbitrary post-training Q4 is equivalent. |

## Decode acceleration

| Source | Evidence | Mechanism | Medousa order of operations |
|---|---|---|---|
| [Speculative decoding in llama.cpp](https://github.com/ggml-org/llama.cpp/blob/master/docs/speculative.md) | Upstream | Draft model or n-gram candidates are verified by the target model | Test n-gram first because it adds little resident memory and may suit code/repetition. |
| [Medusa](https://arxiv.org/abs/2401.10774) | Research | Multiple trained decode heads propose a tree of future tokens | Attractive for a Medousa-tuned model family, but requires checkpoint-specific training and engine support. |
| [EAGLE-3](https://arxiv.org/abs/2503.01840) | Research | A learned draft uses fused multi-layer target features; paper reports large single-request speedups and more modest batched throughput gains | Candidate after a stable target model and training/evaluation pipeline exist. Acceptance rate, memory, and target-device kernels decide viability. |
| [DSpark](https://arxiv.org/abs/2607.05147) | Research | A deep parallel drafter plus lightweight sequential head reduces suffix-decay; a calibrated confidence head and hardware-aware scheduler choose how much of each draft block to verify | High-priority experiment after baseline speculation. Separate the batch-one value of the semi-autoregressive drafter from the high-concurrency value of confidence scheduling. |
| [DeepSpec](https://github.com/deepseek-ai/DeepSpec) | Upstream research code | MIT-licensed training/evaluation code for DSpark, DFlash, and EAGLE-3 plus released DSpark checkpoints for Qwen3 4B/8B/14B and Gemma 4 12B | Reproduce against one candidate worker model first; treat each drafter as target-, mode-, domain-, and recipe-specific rather than a reusable generic accelerator. |
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
- per-device results across Metal, CUDA, and HIP, including native Windows.

DSpark adds two distinct hypotheses that must not be bundled into one result:

1. **Semi-autoregressive drafting:** a parallel backbone can afford more capacity
   than a token-by-token drafter, while a small Markov/RNN-style head restores
   dependency within the proposed block. This may help Medousa's structured code
   and tool workloads even at concurrency one.
2. **Confidence-scheduled verification:** calibrated prefix-survival estimates
   can avoid verifying low-value suffix tokens using an engine/device-specific
   tokens-per-step curve. This is likely most valuable under concurrency and
   must beat a simpler fixed or adaptive draft length on desktop.

Promotion requires exact target-distribution tests, batch-one and burst-load
measurements, confidence calibration by domain, draft/target combined memory,
load and unload cost, and a native implementation on each promoted backend.
DeepSpec's released checkpoints were trained against named target models in
non-thinking mode; thinking, coding, or Medousa-specific use may require a new
draft fine-tune and is not inferred from the paper's production result.

## Alternative generation processes

Autoregressive next-token decoding is a baseline, not a permanent product
boundary. Alternative generation families may expose revision, parallel canvas,
bidirectional, recurrent, continuous, or discrete latent processes. MIR studies
their product semantics as well as their kernel speed.

| Source | Evidence | Mechanism | Medousa question |
|---|---|---|---|
| [DiffusionGemma overview](https://ai.google.dev/gemma/docs/diffusiongemma) | Upstream | A cached autoregressive prompt encoder plus a bidirectional diffusion decoder that iteratively denoises a 256-token canvas; completed canvases are added back to the cache for block-autoregressive continuation | Can parallel canvas refinement improve low-batch local code, editing, structured output, or reflex work after counting memory, quality, commit latency, and non-append-only UI semantics? |
| [DiffusionGemma launch](https://blog.google/innovation-and-ai/technology/developers-tools/diffusion-gemma-faster-text-generation/) | Upstream measured release | Google reports up to 4x generation speed on dedicated GPUs for a 25.2B-total/3.8B-active MoE, while documenting lower quality than autoregressive Gemma 4 and weaker expected acceleration on Apple unified-memory systems | Reproduce per accelerator. Do not transfer CUDA speed claims to Metal or ROCm, and do not hide quality/energy behind tokens per second. |
| [Gemini Diffusion](https://deepmind.google/models/gemini-diffusion/) | Upstream research program | Parallel iterative text refinement with control over intermediate text structure | Which interaction patterns benefit from revisable text rather than left-to-right streaming? |

Diffusion changes the interface contract. A worker may revise any uncommitted
canvas position, then atomically commit a span. Benchmarks therefore record time
to first useful preview, time to first committed span, revision count, final
completion latency, cancellation points, and user-visible stability—not a single
autoregressive TTFT/tokens-per-second number.

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
| R-01 | Which engine is best for each platform/device/model/format? | Identical model family and quality target across the qualified Metal, CUDA, HIP, Vulkan, and CPU candidates | Pareto frontier for quality, TTFT, decode, peak memory, unload recovery, and energy per device class |
| R-02 | Does pre-quantized UQFF avoid the current load spike? | Compare UQFF load with in-situ quantization under process physical-footprint sampling | Lower peak without quality or steady-speed regression |
| R-03 | Is GGUF the best cross-platform distribution format? | Same target model at comparable effective bits on Metal, CPU, CUDA, HIP, and Vulkan | Operational simplicity plus competitive Pareto result |
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
| R-14 | Where are custom accelerator kernels justified? | Profile top operations after engine/recipe optimization on Metal, CUDA, and HIP | Stable hotspot with no acceptable upstream implementation, a testable kernel contract, and a portable fallback |
| R-15 | Does a native-Metal runtime change our build-vs-integrate boundary? | Reproduce BaseRT and Uzu against MLX, llama.cpp, and mistral.rs on identical artifacts and Macs | Meaningful quality-equivalent product gain plus acceptable license, packaging, lifecycle, and maintainability; proprietary BaseRT results may guide but cannot silently become core infrastructure |
| R-16 | Can speculative cascades unify routing and decode acceleration? | Reflex/worker pair with calibrated deferral confidence and target verification | Better quality/latency frontier than plain routing and plain speculative decoding, including both resident footprints |
| R-17 | Can TurboQuant-class compression help local KV and retrieval together? | Reference implementation on long-context tasks and representative vector indexes | Lower combined memory with negligible task/recall loss and a fast consumer-hardware kernel |
| R-18 | What is the CUDA desktop engine frontier? | llama.cpp CUDA versus qualified mistral.rs, TensorRT-LLM, vLLM, and SGLang recipes on identical NVIDIA hardware/artifacts | A native Linux and Windows package winner by device class, including load/unload and batch-one latency |
| R-19 | What is the ROCm/HIP desktop engine frontier? | llama.cpp HIP versus qualified vLLM/SGLang and portable alternatives on supported Radeon and Instinct targets | A Linux winner per `gfx` class with measured kernel coverage, memory recovery, and driver envelope |
| R-20 | What is the dependable Windows AMD lane? | Native HIP on a jointly supported release versus Vulkan, DirectML/ONNX Runtime, and CPU | Safe automatic selection that never assumes Linux ROCm parity and always has a tested fallback |
| R-21 | Do backend-neutral semantics remain equivalent? | Cross-backend golden tests for tokenization, templates, sampling, grammars, stop conditions, cancellation, cache identity, and tool calls | No silent quality or behavioral change when the resolver chooses a different accelerator |
| R-22 | Which quantization/kernel pair wins per accelerator generation? | Quality-equivalent format sweeps across representative Metal families, NVIDIA compute capabilities, and AMD `gfx` targets | Recipe selection by measured end-to-end Pareto result, never format popularity or nominal bit width |
| R-23 | Does DSpark improve Medousa's single-user frontier? | Reproduce one released target/drafter pair; compare fixed block, confidence scheduling, n-gram, classic draft, DFlash, and EAGLE-3 at batch 1 plus short bursts on Metal, CUDA, and HIP where implementable | Quality-equivalent p50/p95 speedup after combined memory/load/energy cost; calibrated scheduling must independently beat simpler adaptive length |
| R-24 | Where does diffusion text generation beat autoregression? | DiffusionGemma versus quality- and footprint-aware autoregressive Gemma recipes on editing, code infill, structured blocks, chat, and tool calls across qualified CUDA, ROCm, and Metal engines | Product-weighted win in at least one declared capability lane, including preview/commit latency, revision UX, memory, energy, quality, and cancellation |

## Maintenance cadence

- Re-run the engine matrix for major engine releases, new Apple GPU families,
  NVIDIA compute capabilities, AMD `gfx` targets, driver/runtime changes, and
  every catalog model change.
- Re-validate native Windows and Linux separately. A CUDA or ROCm result on one
  operating system does not promote a recipe on the other.
- Review this source ledger monthly while the epic is active, then at least once
  per Medousa release train.
- Record negative results. A technique rejected on M1 may remain viable on M5,
  and a technique that wins throughput may still lose interactive latency.
- Never silently replace a model artifact. Catalog entries are immutable by
  digest; new quantizations receive new recipe revisions.
