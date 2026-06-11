<script lang="ts">
  import { Brain, ChevronRight } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
</script>

<div class="flex h-full flex-col">
  <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">Step 1 of 3</p>
  <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold text-surface-50">
    Welcome to Medousa
  </h1>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    I'm your second brain — always here, always yours, always private. First, let's decide how I
    should think.
  </p>

  <div class="mt-6 rounded-xl border border-primary-500/40 bg-primary-500/10 p-5">
    <div class="flex items-start gap-3">
      <Brain class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
      <div class="min-w-0">
        <p class="font-semibold text-surface-50">Bring your own model</p>
        <p class="mt-1 text-sm text-surface-300">
          Phase C wires provider cards, Ollama auto-detect, and API key validation here. For now,
          continue with your existing workshop or set it up in Settings → Voice.
        </p>
        {#if wizard.existingProvider}
          <p class="mt-3 text-xs text-primary-200">
            Detected: {wizard.existingProvider}
            {#if wizard.existingModel}
              · {wizard.existingModel}
            {/if}
          </p>
        {/if}
      </div>
    </div>
  </div>

  <p class="workshop-faint mt-4 text-xs">
    Recommended managed AI and offline bundles land in Phase C/E — skip-friendly from day one.
  </p>

  <div class="mt-auto flex justify-end pt-8">
    <button
      type="button"
      class="btn variant-filled-primary inline-flex min-h-11 items-center gap-2 px-6"
      disabled={wizard.busy}
      onclick={() => void wizard.continue(wizard.existingProvider ?? "byok")}
    >
      Continue
      <ChevronRight class="h-4 w-4" aria-hidden="true" />
    </button>
  </div>
</div>
