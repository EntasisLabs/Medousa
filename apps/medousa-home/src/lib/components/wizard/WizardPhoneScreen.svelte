<script lang="ts">
  import { Smartphone } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
</script>

<div class="flex h-full flex-col">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <p class="mt-4 text-[11px] font-semibold uppercase tracking-wide text-primary-300">Step 3 of 3</p>
  <h2 class="mt-2 text-2xl font-semibold text-surface-50">Add your phone</h2>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Talk to Medousa from anywhere in your home — your phone becomes a second portal to your brain.
  </p>

  <div
    class="mt-6 flex flex-col items-center rounded-xl border border-dashed border-surface-500/45 bg-surface-950/50 px-6 py-10 text-center"
  >
    <Smartphone class="h-10 w-10 text-surface-500" aria-hidden="true" />
    <p class="mt-4 text-sm text-surface-300">
      QR pairing lands in Phase D — live <span class="font-mono text-xs">GET /qr</span> from Medousa
      Core is already wired.
    </p>
    <p class="workshop-faint mt-2 text-xs">Same Wi‑Fi makes discovery instant.</p>
  </div>

  <div class="mt-auto flex flex-wrap items-center justify-between gap-3 pt-8">
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy}
      onclick={() => void wizard.skipCurrent()}
    >
      I'll do this later
    </button>
    <button
      type="button"
      class="btn variant-filled-primary min-h-11 px-6"
      disabled={wizard.busy}
      onclick={() => void wizard.continue()}
    >
      Finish setup
    </button>
  </div>
</div>
