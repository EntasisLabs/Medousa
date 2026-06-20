<script lang="ts">
  import { settings, COLOR_THEME_OPTIONS } from "$lib/stores/settings.svelte";
  import { COLOR_THEME_GROUP_LABELS, COLOR_THEME_GROUPS } from "$lib/types/colorThemes";
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Room</h2>
    <p class="workshop-faint mt-1 text-sm">
      The atmosphere of your workshop — palette and light.
    </p>
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

  <div class="settings-toggle-list mt-6">
    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Dark mode</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Use the dark variant of your chosen palette
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.darkMode}
        onchange={(event) =>
          settings.setDarkMode((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>
  </div>
</section>
