<script lang="ts">
  import { wizard } from "$lib/stores/wizard.svelte";
  import WizardMigrationScreen from "$lib/components/wizard/WizardMigrationScreen.svelte";
  import WizardWelcomeScreen from "$lib/components/wizard/WizardWelcomeScreen.svelte";
  import WizardAccountScreen from "$lib/components/wizard/WizardAccountScreen.svelte";
  import WizardPhoneScreen from "$lib/components/wizard/WizardPhoneScreen.svelte";
  import WizardCompletionScreen from "$lib/components/wizard/WizardCompletionScreen.svelte";

  const reducedMotion =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;
</script>

<div
  class="fixed inset-0 z-[100] flex items-center justify-center bg-surface-950/95 p-4 backdrop-blur-md"
  role="presentation"
>
  <div
    class="flex h-[min(720px,92vh)] w-full max-w-[640px] flex-col overflow-hidden rounded-2xl border border-surface-500/40 bg-surface-900 shadow-2xl"
    role="dialog"
    aria-modal="true"
    aria-labelledby="product-wizard-title"
  >
    {#if wizard.error}
      <div class="border-b border-error-500/30 bg-error-500/10 px-5 py-3 text-sm text-error-200">
        {wizard.error}
      </div>
    {/if}

    <div
      class="min-h-0 flex-1 overflow-y-auto px-6 py-6"
      class:wizard-crossfade={!reducedMotion}
    >
      {#key wizard.screen}
        {#if wizard.screen === "migration"}
          <WizardMigrationScreen />
        {:else if wizard.screen === "screen1"}
          <WizardWelcomeScreen />
        {:else if wizard.screen === "screen2"}
          <WizardAccountScreen />
        {:else if wizard.screen === "screen3"}
          <WizardPhoneScreen />
        {:else}
          <WizardCompletionScreen />
        {/if}
      {/key}
    </div>
  </div>
</div>

<style>
  .wizard-crossfade {
    animation: wizard-fade-in 300ms ease;
  }

  @keyframes wizard-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .wizard-crossfade {
      animation: none;
    }
  }
</style>
