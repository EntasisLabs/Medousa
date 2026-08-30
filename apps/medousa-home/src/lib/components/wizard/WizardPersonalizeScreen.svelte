<script lang="ts">
  import { onMount, tick } from "svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import MedousaMark from "$lib/components/brand/MedousaMark.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { isTauri } from "$lib/window";
  import {
    MEDOUSA_MARK_OPTIONS,
    medousaMarkOption,
    type MedousaMarkId,
  } from "$lib/theme/medousaMarks";
  import {
    loadPrincipalName,
    saveAssistantName,
    savePrincipalName,
  } from "$lib/utils/onboardingIdentity";

  let spaceName = $state("");
  let yourName = $state(loadPrincipalName());
  let selectedMark = $state<MedousaMarkId>(settings.medousaMark);
  let saving = $state(false);
  let nameInput: HTMLInputElement | undefined = $state();
  let markPulse = $state(0);

  const mobile = isTauriMobilePlatform();

  const selectedMarkOption = $derived(medousaMarkOption(selectedMark));
  const accent = $derived(
    settings.darkMode ? selectedMarkOption.darkColor : selectedMarkOption.lightColor,
  );

  onMount(() => {
    void (async () => {
      if (isTauri()) {
        try {
          await workshops.load();
          const label = workshops.activeLabel?.trim();
          if (label && label !== "Personal") {
            spaceName = label;
          } else if (mobile) {
            spaceName = "Home";
          }
        } catch {
          /* keep empty for placeholder */
        }
      }
      await tick();
      // Do not cover a mobile setup screen with the keyboard before the user
      // has chosen to edit anything.
      if (!mobile) {
        nameInput?.focus();
        if (spaceName) nameInput?.select();
      }
    })();
  });

  function pickMark(id: MedousaMarkId) {
    const option = medousaMarkOption(id);
    selectedMark = id;
    settings.setMedousaMark(id);
    settings.setColorTheme(option.pairedThemeId, { persistWorkshop: false });
    markPulse += 1;
  }

  async function continueSpace() {
    saving = true;
    wizard.error = null;
    try {
      const label = spaceName.trim() || "Home";
      if (isTauri() && workshops.activeWorkshopId) {
        try {
          if (label !== workshops.activeLabel) {
            await workshops.renameWorkshop(workshops.activeWorkshopId, label);
          }
          await workshops.updateBranding(workshops.activeWorkshopId, {
            brandColor: accent,
            tagline: null,
          });
        } catch {
          /* local theme still applied */
        }
      }

      settings.setMedousaMark(selectedMark);
      settings.setColorTheme(selectedMarkOption.pairedThemeId);
      savePrincipalName(yourName);
      saveAssistantName("Medousa");
      wizard.completeSpace();
    } catch (err) {
      wizard.error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<div class="wizard-step">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy || saving}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <div class="wizard-stagger flex min-h-0 flex-1 flex-col items-center justify-center px-2 text-center">
    <p class="wizard-beat text-[11px] font-semibold uppercase tracking-[0.16em] text-content-link/90">
      {mobile ? "Your Home" : "Your space"}
    </p>
    <h1 id="product-wizard-title" class="wizard-beat mt-2 text-2xl font-semibold tracking-tight text-surface-50">
      Make it yours
    </h1>
    <p class="wizard-beat mt-2 max-w-sm text-sm text-content-tertiary">
      Give it a name, pick a look, and tell Medousa what to call you.
    </p>

    {#key markPulse}
      <div
        class="wizard-beat desk-mark mt-8 flex h-28 w-28 items-center justify-center rounded-3xl border-2 p-5"
        style:border-color={accent}
        style:background={settings.darkMode
          ? selectedMarkOption.darkPreviewBackground
          : selectedMarkOption.lightPreviewBackground}
        aria-hidden="true"
      >
        <MedousaMark markId={selectedMark} darkMode={settings.darkMode} decorative />
      </div>
    {/key}

    <div class="wizard-beat mt-7 w-full max-w-xs">
      <input
        bind:this={nameInput}
        class="space-name-input w-full text-center"
        bind:value={spaceName}
        maxlength={48}
        placeholder={mobile ? "Home name" : "Workspace name"}
        aria-label={mobile ? "Home name" : "Workspace name"}
        disabled={wizard.busy || saving}
      />
    </div>

    <p class="wizard-beat mt-7 text-xs font-medium uppercase tracking-wide text-content-quiet">
      {selectedMarkOption.label}
    </p>
    <div class="wizard-beat mt-3 grid grid-cols-5 gap-2.5">
      {#each MEDOUSA_MARK_OPTIONS as option (option.id)}
        <button
          type="button"
          class="mark-choice {selectedMark === option.id ? 'mark-choice-active' : ''}"
          style:--mark-choice-bg={settings.darkMode
            ? option.darkPreviewBackground
            : option.lightPreviewBackground}
          title={option.label}
          aria-label={option.label}
          aria-pressed={selectedMark === option.id}
          disabled={wizard.busy || saving}
          onclick={() => pickMark(option.id)}
        >
          <span><MedousaMark markId={option.id} darkMode={settings.darkMode} simplified decorative /></span>
        </button>
      {/each}
    </div>

    <label class="wizard-beat mt-8 block w-full max-w-xs">
      <span class="sr-only">Profile name (optional)</span>
      <input
        class="your-name-input w-full text-center"
        bind:value={yourName}
        maxlength={40}
        placeholder="Profile Name (Optional)"
        disabled={wizard.busy || saving}
      />
    </label>
  </div>

  <div class="flex justify-center pt-4">
    <button
      type="button"
      class="btn variant-filled-primary wizard-cta min-h-12 px-12"
      disabled={wizard.busy || saving || !spaceName.trim()}
      onclick={() => void continueSpace()}
    >
      This feels right
    </button>
  </div>
</div>

<style>
  .desk-mark {
    animation: desk-mark-in 420ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  @keyframes desk-mark-in {
    from {
      opacity: 0.65;
      transform: scale(0.94);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .space-name-input {
    border: none;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: transparent;
    padding: 0.6rem 0.25rem;
    font-size: 1.45rem;
    font-weight: 600;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
    outline: none;
  }

  .space-name-input:focus {
    border-bottom-color: rgb(var(--color-primary-400) / 0.75);
  }

  .your-name-input {
    border: none;
    background: transparent;
    padding: 0.4rem;
    font-size: 0.875rem;
    color: rgb(var(--theme-text-secondary));
    outline: none;
  }

  .your-name-input::placeholder {
    color: rgb(var(--theme-text-faint));
  }

  .mark-choice {
    display: grid;
    width: 2.75rem;
    height: 2.75rem;
    place-items: center;
    border-radius: 0.8rem;
    border: 2px solid rgb(var(--color-surface-500) / 0.35);
    background: var(--mark-choice-bg);
    transition:
      transform 160ms ease,
      border-color 160ms ease;
  }

  .mark-choice > span {
    width: 1.55rem;
    height: 1.55rem;
  }

  .mark-choice:hover:not(:disabled) {
    transform: scale(1.08);
  }

  .mark-choice-active {
    border-color: rgb(var(--color-surface-50) / 0.9);
    transform: scale(1.08);
  }

  @media (prefers-reduced-motion: reduce) {
    .desk-mark {
      animation: none;
    }

    .mark-choice {
      transition: none;
    }
  }
</style>
