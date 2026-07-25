<script lang="ts">
  import RoomShellOptions from "$lib/components/settings/RoomShellOptions.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { settings, COLOR_THEME_OPTIONS } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { COLOR_THEME_GROUP_LABELS, COLOR_THEME_GROUPS } from "$lib/types/colorThemes";
  import { presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { isTauri } from "$lib/window";
  import { Moon, Sun } from "@lucide/svelte";

  const activePreset = $derived(
    environment.spec?.layoutPresets?.find((preset) => preset.active) ??
      environment.spec?.layoutPresets?.find(
        (preset) => preset.id === environment.spec?.activePresetId,
      ) ??
      null,
  );
  const activeLayoutLabel = $derived(
    presetDisplayLabel(activePreset?.id ?? "default", activePreset?.label),
  );

  const roomHint = $derived.by(() => {
    if (!isTauri()) {
      return "Atmosphere, chrome, and how this space looks.";
    }
    return `${workshops.activeLabel} — theme, light, and shell chrome.`;
  });

  let themeBusy = $state(false);

  function toggleDarkMode() {
    settings.setDarkMode(!settings.darkMode);
  }

  async function pickTheme(themeId: (typeof COLOR_THEME_OPTIONS)[number]["id"]) {
    if (themeBusy || settings.colorTheme === themeId) return;
    themeBusy = true;
    try {
      await environment.setActiveLayoutColorTheme(themeId);
    } catch {
      // Fall back to local shell theme if the env save fails.
      settings.setColorTheme(themeId);
    } finally {
      themeBusy = false;
    }
  }
</script>

<section class="settings-section">
  <header class="settings-section-header room-section-header">
    <div class="min-w-0 flex-1">
      <h2 class="text-base font-semibold text-surface-50">Room</h2>
      <p class="workshop-faint mt-1 text-sm">
        {roomHint}
      </p>
    </div>
    <button
      type="button"
      class="workshop-rail-btn room-theme-toggle shrink-0"
      aria-label={settings.darkMode ? "Switch to light mode" : "Switch to dark mode"}
      title={settings.darkMode ? "Light mode" : "Dark mode"}
      aria-pressed={settings.darkMode}
      onclick={toggleDarkMode}
    >
      {#if settings.darkMode}
        <Sun size={16} strokeWidth={1.75} />
      {:else}
        <Moon size={16} strokeWidth={1.75} />
      {/if}
    </button>
  </header>

  <p class="workshop-faint room-theme-layout-hint mt-6">
    Theme for layout <span class="room-theme-layout-name">{activeLayoutLabel}</span>
    — switches with the status-bar layout menu.
  </p>

  {#each COLOR_THEME_GROUPS as group (group)}
    <p class="workshop-label mt-4">{COLOR_THEME_GROUP_LABELS[group]}</p>
    <div class="mt-2 grid gap-2 lg:grid-cols-2">
      {#each COLOR_THEME_OPTIONS.filter((option) => option.group === group) as option (option.id)}
        <button
          type="button"
          class="theme-option {settings.colorTheme === option.id ? 'theme-option-active' : ''}"
          aria-pressed={settings.colorTheme === option.id}
          disabled={themeBusy}
          onclick={() => void pickTheme(option.id)}
        >
          <div class="theme-option-swatches" aria-hidden="true">
            {#each option.swatches as swatch, index (index)}
              <span style:background-color={swatch}></span>
            {/each}
          </div>
          <div class="min-w-0 text-left">
            <p class="text-sm font-medium text-surface-100">{option.label}</p>
            <p class="workshop-faint mt-0.5 leading-snug">{option.tagline}</p>
          </div>
        </button>
      {/each}
    </div>
  {/each}

  <h3 class="settings-subsection-heading mt-8">Shell</h3>
  <p class="settings-subsection-lead">Rail chrome and phone Home for this profile.</p>
  <RoomShellOptions />
</section>

<style>
  .room-section-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    width: 100%;
    max-width: none;
  }

  .room-theme-toggle {
    margin-top: 0.05rem;
    margin-inline-start: auto;
  }

  .room-theme-layout-hint {
    margin: 0;
    font-size: 0.75rem;
  }

  .room-theme-layout-name {
    font-weight: 600;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }
</style>
