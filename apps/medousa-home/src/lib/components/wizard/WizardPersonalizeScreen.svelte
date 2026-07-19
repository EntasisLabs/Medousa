<script lang="ts">
  import { onMount, tick } from "svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/window";
  import {
    COLOR_THEME_OPTIONS,
    type ColorThemeId,
  } from "$lib/theme/themeRegistry";
  import { workshopMonogram } from "$lib/types/workshopRegistry";
  import {
    loadPrincipalName,
    saveAssistantName,
    savePrincipalName,
  } from "$lib/utils/onboardingIdentity";

  const themeChoices = COLOR_THEME_OPTIONS.filter(
    (theme) =>
      theme.group === "workshop" ||
      theme.id === "cupertino" ||
      theme.id === "nord" ||
      theme.id === "tokyo-night",
  );

  let spaceName = $state("");
  let yourName = $state(loadPrincipalName());
  let selectedTheme = $state<ColorThemeId>(settings.colorTheme);
  let saving = $state(false);
  let nameInput: HTMLInputElement | undefined = $state();
  let markPulse = $state(0);

  const monogram = $derived(workshopMonogram(spaceName.trim() || "Workspace"));
  const accent = $derived(
    themeChoices.find((t) => t.id === selectedTheme)?.swatches[1] ?? "#7C3AED",
  );
  const themeLabel = $derived(
    themeChoices.find((t) => t.id === selectedTheme)?.label ?? "Theme",
  );

  onMount(() => {
    void (async () => {
      if (isTauri()) {
        try {
          await workshops.load();
          const label = workshops.activeLabel?.trim();
          if (label && label !== "Personal") spaceName = label;
        } catch {
          /* keep empty for placeholder */
        }
      }
      await tick();
      nameInput?.focus();
      if (spaceName) nameInput?.select();
    })();
  });

  function pickTheme(id: ColorThemeId) {
    selectedTheme = id;
    settings.setColorTheme(id);
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

      settings.setColorTheme(selectedTheme);
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
    <p class="wizard-beat text-[11px] font-semibold uppercase tracking-[0.16em] text-primary-300/90">
      Your space
    </p>
    <h1 id="product-wizard-title" class="wizard-beat mt-2 text-2xl font-semibold tracking-tight text-surface-50">
      Make it yours
    </h1>
    <p class="wizard-beat mt-2 max-w-sm text-sm text-surface-400">
      Add a name and pick your theme.
    </p>

    {#key markPulse}
      <div
        class="wizard-beat desk-mark mt-8 flex h-28 w-28 items-center justify-center rounded-full border-2 bg-surface-950/85 text-[2.4rem] font-semibold tracking-tight text-surface-50"
        style:border-color={accent}
        aria-hidden="true"
      >
        {monogram}
      </div>
    {/key}

    <div class="wizard-beat mt-7 w-full max-w-xs">
      <input
        bind:this={nameInput}
        class="space-name-input w-full text-center"
        bind:value={spaceName}
        maxlength={48}
        placeholder="Workspace Name"
        aria-label="Workspace name"
        disabled={wizard.busy || saving}
      />
    </div>

    <p class="wizard-beat mt-7 text-xs font-medium uppercase tracking-wide text-surface-500">
      {themeLabel}
    </p>
    <div class="wizard-beat mt-3 flex flex-wrap justify-center gap-2.5">
      {#each themeChoices as theme (theme.id)}
        <button
          type="button"
          class="theme-swatch {selectedTheme === theme.id ? 'theme-swatch-active' : ''}"
          style:--swatch-a={theme.swatches[0]}
          style:--swatch-b={theme.swatches[1]}
          style:--swatch-c={theme.swatches[2]}
          title={theme.label}
          aria-label={theme.label}
          aria-pressed={selectedTheme === theme.id}
          disabled={wizard.busy || saving}
          onclick={() => pickTheme(theme.id)}
        ></button>
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
    color: rgb(var(--color-surface-300));
    outline: none;
  }

  .your-name-input::placeholder {
    color: rgb(var(--color-surface-600));
  }

  .theme-swatch {
    width: 2.1rem;
    height: 2.1rem;
    border-radius: 9999px;
    border: 2px solid rgb(var(--color-surface-500) / 0.35);
    background: linear-gradient(
      135deg,
      var(--swatch-a) 0%,
      var(--swatch-b) 55%,
      var(--swatch-c) 100%
    );
    transition:
      transform 160ms ease,
      border-color 160ms ease;
  }

  .theme-swatch:hover:not(:disabled) {
    transform: scale(1.08);
  }

  .theme-swatch-active {
    border-color: rgb(var(--color-surface-50) / 0.9);
    transform: scale(1.08);
  }

  @media (prefers-reduced-motion: reduce) {
    .desk-mark {
      animation: none;
    }

    .theme-swatch {
      transition: none;
    }
  }
</style>
