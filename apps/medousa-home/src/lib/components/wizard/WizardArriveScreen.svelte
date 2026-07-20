<script lang="ts">
  import { wizard } from "$lib/stores/wizard.svelte";
  import { hostComputerPhrase } from "$lib/platformCopy";

  const hostPhrase = hostComputerPhrase();
</script>

<div class="wizard-step wizard-stagger items-center justify-center text-center">
  <div
    class="wizard-beat flex h-[4.5rem] w-[4.5rem] items-center justify-center rounded-2xl border border-primary-500/35 bg-surface-950/80 text-[1.75rem] font-semibold tracking-tight text-surface-50"
    aria-hidden="true"
  >
    M
  </div>

  <h1
    id="product-wizard-title"
    class="wizard-beat mt-8 text-3xl font-semibold tracking-tight text-surface-50"
  >
    Medousa
  </h1>

  <p class="wizard-beat mt-4 max-w-sm text-lg leading-snug text-surface-200">
    Your permanent workspace on {hostPhrase}.
  </p>
  <p class="wizard-beat mt-2 max-w-sm text-sm leading-relaxed text-surface-500">
    Notes and files stay here. No account.
  </p>

  <div class="wizard-beat mt-12">
    <button
      type="button"
      class="btn variant-filled-primary wizard-cta min-h-12 px-12"
      disabled={wizard.busy}
      onclick={() => wizard.completeArrive()}
    >
      Begin
    </button>
  </div>
</div>
