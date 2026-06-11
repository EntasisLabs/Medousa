<script lang="ts">
  import { Sparkles } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
</script>

<div class="flex h-full flex-col">
  <div class="flex items-center gap-2 text-primary-300">
    <Sparkles class="h-5 w-5" aria-hidden="true" />
    <p class="text-[11px] font-semibold uppercase tracking-wide">Welcome to the new Medousa</p>
  </div>

  <h1 id="product-wizard-title" class="mt-4 text-2xl font-semibold text-surface-50">
    Your workshop, now with a front door.
  </h1>

  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    We found an existing setup on this machine — likely from the terminal wizard or an earlier
    build. Nothing was reset. Your memories, manuscripts, and workshop files are exactly where
    you left them.
  </p>

  <ul class="mt-6 space-y-3 text-sm text-surface-200">
    <li class="rounded-lg border border-surface-500/35 bg-surface-950/50 px-4 py-3">
      GUI-first onboarding replaces <span class="font-mono text-xs">medousa setup</span> for new installs.
    </li>
    <li class="rounded-lg border border-surface-500/35 bg-surface-950/50 px-4 py-3">
      Phone pairing and managed AI paths arrive in the next screens when you choose them.
    </li>
    <li class="rounded-lg border border-surface-500/35 bg-surface-950/50 px-4 py-3">
      Medousa Core keeps running lean — durable workers, same daemon you already trust.
    </li>
  </ul>

  {#if wizard.existingProvider}
    <p class="workshop-faint mt-5 text-xs">
      Current brain: {wizard.existingProvider}
      {#if wizard.existingModel}
        · {wizard.existingModel}
      {/if}
    </p>
  {/if}

  <div class="mt-auto flex justify-end pt-8">
    <button
      type="button"
      class="btn variant-filled-primary min-h-11 px-6"
      disabled={wizard.busy}
      onclick={() => void wizard.continue()}
    >
      Continue to Home
    </button>
  </div>
</div>
