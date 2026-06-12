<script lang="ts">
  import { Sparkles } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
</script>

<div class="flex h-full flex-col">
  <div class="flex items-center gap-2 text-primary-300">
    <Sparkles class="h-5 w-5" aria-hidden="true" />
    <p class="text-[11px] font-semibold uppercase tracking-wide">Welcome back</p>
  </div>

  <h1 id="product-wizard-title" class="mt-4 text-2xl font-semibold text-surface-50">
    Everything is still here.
  </h1>

  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Medousa found your existing setup on this computer. Your notes, memories, and conversations
    were not reset.
  </p>

  {#if wizard.existingProvider}
    <p class="workshop-faint mt-5 text-xs">
      Current model: {wizard.existingProvider}
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
      Continue
    </button>
  </div>
</div>
