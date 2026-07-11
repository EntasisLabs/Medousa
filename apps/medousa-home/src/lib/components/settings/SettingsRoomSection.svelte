<script lang="ts">
  import { settings, COLOR_THEME_OPTIONS } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { COLOR_THEME_GROUP_LABELS, COLOR_THEME_GROUPS } from "$lib/types/colorThemes";
  import { isTauri } from "$lib/window";
  import { Moon, Sun } from "@lucide/svelte";

  const roomHint = $derived.by(() => {
    if (!isTauri()) {
      return "The atmosphere of your workshop — palette and light.";
    }
    return `Palette for ${workshops.activeLabel} — switches when you change workshops.`;
  });

  function toggleDarkMode() {
    settings.setDarkMode(!settings.darkMode);
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

  {#each COLOR_THEME_GROUPS as group (group)}
    <p class="workshop-label mt-6">{COLOR_THEME_GROUP_LABELS[group]}</p>
    <div class="mt-2 grid gap-2 lg:grid-cols-2">
      {#each COLOR_THEME_OPTIONS.filter((option) => option.group === group) as option (option.id)}
        <button
          type="button"
          class="theme-option {settings.colorTheme === option.id ? 'theme-option-active' : ''}"
          aria-pressed={settings.colorTheme === option.id}
          onclick={() => settings.setColorTheme(option.id)}
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
</style>
