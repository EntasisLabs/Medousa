# Packages (Settings)

**Audience:** Medousa app users who want optional binaries without opening the
standalone installer.

Home already ships with the **engine** (daemon, CLI, and TUI). Packages is where
you add more later — offline brain, messaging adapters, MCP gateway.

From a terminal you can also use:

```bash
medousa packages status
medousa pull mcp-gateway
medousa pull telegram
medousa update
```

---

## Open Packages

1. Open **Settings** (gear).
2. Choose **Packages** in the left nav (desktop), or find **Packages** under
   Connection → Extras on some layouts.

You should see a short list of optional components with **Install**, **Update**,
or **Installed**.

---

## What you can install

| Package | What you get |
|---------|----------------|
| **Offline brain** | `medousa_local` — on-device inference for Gemma |
| **MCP gateway** | Connect MCP tool servers to Medousa |
| **Telegram / Discord / Slack / WhatsApp** | Channel adapter binaries |

**Not listed here (on purpose):**

- The **desktop app** — you’re already in it
- The **engine** — bundled with Home
- **Model weights** — download from the private-brain / Models UI after Offline
  brain is installed

---

## Install or update

1. Click **Install** (or **Update** when a newer build is available).
2. Wait for the progress line — one install at a time is enough.
3. When it says **Installed**, the binary is under your Medousa data directory
   (`…/medousa/bin`). Home finds it automatically.

Need model weights next? Use the link to **Connection → Extras** (private brain
panel) or **Settings → Models**, then download Gemma.

Downloading prepares the model but does not load it into memory. Medousa starts
the local inference worker when the first chat turn targets **Medousa Local**.
To release the model memory immediately, open **Settings → Connection → Private
brain** and choose the power button. The next local turn loads it again.

Before loading, Medousa keeps at least 4 GiB or 25% of system memory—whichever
is larger—available for the OS and other apps. When accelerator memory counters
are available, it also reserves at least 512 MiB or 10% of that device's
capacity and requires the model to fit both envelopes. If the chosen model
cannot fit, it stays cold and Medousa explains which envelope refused it instead
of trying the allocation. Missing device counters are reported explicitly and
fall back to host-only admission. Automatic recommendations prefer the
strongest smaller model that fits right now. An idle local worker exits after
five minutes. During model loading, Medousa also holds a short-lived per-user
activation reservation. This prevents two workers or workshops from both
claiming the same not-yet-allocated headroom at once; crashed-process
reservations are reclaimed automatically.

### Benchmark an installed local model

Developers can capture a content-free lifecycle manifest for the current engine:

```bash
cargo run -p medousa-local-inference \
  --features embedded-inference-metal \
  --bin medousa_local_bench -- \
  --model-id gemma-4-e2b-it-qat \
  --output local-benchmark.json
```

Use `embedded-inference-cuda` on qualified NVIDIA builds or
`embedded-inference` for the CPU baseline. The command never downloads a model
and refuses an uninstalled or unsafe one. It uses a deterministic synthetic
prompt and records only build/recipe identity, phase timings, response byte
counts, and host/process memory—including reclaimed RSS at 1, 5, and 10 seconds.
Metal builds also record current process allocation and Apple's recommended
working set. NVIDIA systems record VRAM, process VRAM, utilization, power,
temperature, and clocks through `nvidia-smi`; AMD systems normalize the same
available fields from AMD SMI JSON. Unsupported counters remain `null` and are
named in `unavailableFields`; they are never reported as a measured zero. The
shared contract also distinguishes physical memory from a dynamic per-process
budget. Metal working-set, Windows WDDM, and Vulkan budget sources subtract the
current process usage from that budget before admission; physical free VRAM is
not substituted when a stricter budget exists. Native collectors take
precedence over command-line fallbacks for the same device. A reported dynamic
budget without trustworthy process usage fails closed and remains `null` in the
evidence instead of becoming a fabricated zero. The
manifest never stores the prompt or generated content. A successful, complete
run also updates a local peak-calibration record keyed by model, runtime,
quantization, context/batch recipe, CPU architecture, accelerator backend, and
device identity. Admission uses the recorded high-water mark with 15% plus 256
MiB of slack, but never lowers the catalog's static estimate. Failed or
incomplete benchmarks do not change admission.

---

## Remove

Optional packages show **Remove**. That deletes the binary and package marker
from your data directory. Your chats and vault stay put.

---

## Advanced: Medousa Installer

At the bottom of Packages, **Open Medousa Installer…** launches the standalone
installer in modify mode when it’s installed. Use that for repair, full
workloads (Express / Offline workstation / Developer), or headless-oriented
layouts.

Most people never need it after Home-first install.

Operators / CI: [Install & self-host](../cookbook/install-and-self-host.md) ·
[Release to R2](../cookbook/release-to-r2.md).

---

## Tips

- Packages needs a network path to your **release manifest**
  (`MEDOUSA_RELEASE_BASE_URL` or the embedded release defaults).
- Phone / companion apps don’t install desktop binaries — do Packages on the Mac
  or PC that hosts the engine.
- After installing a channel adapter, configure tokens under messaging /
  [Channels](channels.md).
