<script lang="ts">
  import { wizard } from "$lib/stores/wizard.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import WizardMigrationScreen from "$lib/components/wizard/WizardMigrationScreen.svelte";
  import WizardArriveScreen from "$lib/components/wizard/WizardArriveScreen.svelte";
  import WizardPersonalizeScreen from "$lib/components/wizard/WizardPersonalizeScreen.svelte";
  import WizardModeScreen from "$lib/components/wizard/WizardModeScreen.svelte";
  import WizardWelcomeScreen from "$lib/components/wizard/WizardWelcomeScreen.svelte";
  import WizardWelcomeScreenMobile from "$lib/components/wizard/WizardWelcomeScreenMobile.svelte";
  import WizardPhoneScreen from "$lib/components/wizard/WizardPhoneScreen.svelte";
  import WizardCompletionScreen from "$lib/components/wizard/WizardCompletionScreen.svelte";
  import "./wizardExperience.css";

  const reducedMotion =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const mobileShell = isTauriMobilePlatform();

  const phaseKey = $derived(
    mobileShell
      ? wizard.screen
      : wizard.screen === "migration"
        ? "migration"
        : wizard.uiPhase,
  );

  const stepIndex = $derived.by(() => {
    if (mobileShell || wizard.screen === "migration") {
      return { idx: -1, total: 0 };
    }
    const phase = wizard.uiPhase;
    // Packages are post-entry — rail is arrive → space → mode → [brain] → ready.
    const steps =
      wizard.preferredMode === "workspace"
        ? (["arrive", "space", "mode", "ready"] as const)
        : wizard.preferredMode === "workspace-ai"
          ? (["arrive", "space", "mode", "brain", "ready"] as const)
          : (["arrive", "space", "mode"] as const);
    const idx = (steps as readonly string[]).indexOf(phase);
    return { idx, total: steps.length };
  });
</script>

<div class="wizard-backdrop fixed inset-0 z-[100] flex items-center justify-center p-4" role="presentation">
  <div class="wizard-backdrop-wash" aria-hidden="true"></div>

  <div
    class="wizard-panel relative flex flex-col overflow-hidden {mobileShell
      ? 'h-full w-full rounded-none border-0'
      : 'h-[min(720px,92vh)] w-full max-w-[640px] rounded-2xl'}"
    role="dialog"
    aria-modal="true"
    aria-labelledby="product-wizard-title"
  >
    {#if wizard.error}
      <div class="border-b border-error-500/30 bg-error-500/10 px-5 py-3 text-sm text-error-200">
        {wizard.error}
      </div>
    {/if}

    {#if !mobileShell && stepIndex.idx >= 0 && wizard.uiPhase !== "phone"}
      <div class="wizard-progress px-6 pt-5" aria-hidden="true">
        <div class="flex items-center gap-1.5">
          {#each Array(stepIndex.total) as _, i}
            <span
              class="wizard-progress-dot"
              class:wizard-progress-dot-on={i <= stepIndex.idx}
              class:wizard-progress-dot-now={i === stepIndex.idx}
            ></span>
          {/each}
        </div>
      </div>
    {/if}

    <div
      class="min-h-0 flex-1 overflow-y-auto px-6 py-6"
      class:wizard-crossfade={!reducedMotion}
    >
      {#key phaseKey}
        {#if wizard.screen === "migration"}
          <WizardMigrationScreen />
        {:else if mobileShell}
          {#if wizard.screen === "screen1"}
            <WizardWelcomeScreenMobile />
          {:else if wizard.screen === "screen3"}
            <WizardPhoneScreen />
          {:else}
            <WizardCompletionScreen />
          {/if}
        {:else if wizard.uiPhase === "arrive"}
          <WizardArriveScreen />
        {:else if wizard.uiPhase === "space"}
          <WizardPersonalizeScreen />
        {:else if wizard.uiPhase === "mode"}
          <WizardModeScreen />
        {:else if wizard.uiPhase === "brain"}
          <WizardWelcomeScreen />
        {:else if wizard.uiPhase === "phone" || wizard.screen === "screen3"}
          <WizardPhoneScreen />
        {:else}
          <WizardCompletionScreen />
        {/if}
      {/key}
    </div>
  </div>
</div>

<style>
  .wizard-backdrop {
    background: rgb(var(--color-surface-950) / 0.88);
    backdrop-filter: blur(18px);
  }

  .wizard-backdrop-wash {
    pointer-events: none;
    position: absolute;
    inset: 0;
    background:
      radial-gradient(
        900px 480px at 50% -10%,
        rgb(var(--color-primary-500) / 0.14),
        transparent 60%
      ),
      radial-gradient(
        700px 420px at 80% 110%,
        rgb(var(--color-primary-500) / 0.08),
        transparent 55%
      );
  }

  .wizard-panel {
    border: 1px solid rgb(var(--color-surface-500) / 0.38);
    background: rgb(var(--color-surface-900) / 0.96);
    box-shadow:
      0 1px 0 rgb(var(--color-surface-50) / 0.04) inset,
      0 40px 80px -40px rgb(0 0 0 / 0.65);
  }

  .wizard-progress-dot {
    height: 4px;
    flex: 1;
    max-width: 2.5rem;
    border-radius: 9999px;
    background: rgb(var(--color-surface-500) / 0.28);
    transition:
      background 220ms ease,
      max-width 220ms ease;
  }

  .wizard-progress-dot-on {
    background: rgb(var(--color-primary-500) / 0.45);
  }

  .wizard-progress-dot-now {
    max-width: 3.25rem;
    background: rgb(var(--color-primary-400) / 0.9);
  }

  .wizard-crossfade {
    animation: wizard-page-in 420ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  @keyframes wizard-page-in {
    from {
      opacity: 0;
      transform: translateY(16px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .wizard-crossfade {
      animation: none;
    }

    .wizard-progress-dot {
      transition: none;
    }
  }
</style>
