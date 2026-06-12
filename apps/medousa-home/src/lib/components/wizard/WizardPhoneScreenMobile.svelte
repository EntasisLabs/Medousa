<script lang="ts">
  import { ChevronRight, Smartphone } from "@lucide/svelte";
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
  <h2 class="mt-2 text-2xl font-semibold text-surface-50">You're the phone portal</h2>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    This device talks to your Mac over Wi‑Fi. Formal device pairing (QR on the Mac) is optional —
    chat works once Step 1 connected.
  </p>

  <div
    class="mt-6 flex flex-col items-center rounded-xl border border-surface-500/35 bg-surface-950/50 px-6 py-8 text-center"
  >
    <Smartphone class="h-10 w-10 text-primary-300" aria-hidden="true" />
    <p class="mt-4 text-sm text-surface-300">
      To register this phone for LAN pairing later, open <span class="text-surface-100">Medousa on your Mac → Settings → Phone</span>
      and scan from a future phone build — or skip for now.
    </p>
  </div>

  <div class="mt-auto flex justify-end pt-8">
    <button
      type="button"
      class="btn variant-filled-primary inline-flex min-h-11 items-center gap-2 px-6"
      disabled={wizard.busy}
      onclick={() => void wizard.continue()}
    >
      Finish setup
      <ChevronRight class="h-4 w-4" aria-hidden="true" />
    </button>
  </div>
</div>
