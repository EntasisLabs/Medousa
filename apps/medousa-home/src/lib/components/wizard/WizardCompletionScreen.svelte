<script lang="ts">
  import { wizard } from "$lib/stores/wizard.svelte";
</script>

<div class="flex h-full flex-col items-center justify-center text-center">
  <p class="text-3xl" aria-hidden="true">🎉</p>
  <h2 class="mt-4 text-2xl font-semibold text-surface-50">You're ready!</h2>
  <p class="mt-3 max-w-sm text-sm leading-relaxed text-surface-300">
    Medousa Core is running. Your brain is online. Ask me anything when you're back in the
    workshop.
  </p>

  <div
    class="mt-8 w-full max-w-md rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-5 text-left"
  >
    <p class="text-sm text-surface-400">Ask me anything…</p>
  </div>

  <button
    type="button"
    class="btn variant-filled-primary mt-10 min-h-11 px-8"
    disabled={wizard.busy}
    onclick={() => void wizard.finish()}
  >
    Start talking →
  </button>
</div>
