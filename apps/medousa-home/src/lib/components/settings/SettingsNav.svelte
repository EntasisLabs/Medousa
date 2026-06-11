<script lang="ts">
  import type { SettingsSectionId } from "$lib/types/settings";
  import { SETTINGS_SECTIONS } from "$lib/types/settings";

  interface Props {
    active: SettingsSectionId;
    mobile?: boolean;
    onSelect: (section: SettingsSectionId) => void;
  }

  let { active, mobile = false, onSelect }: Props = $props();
</script>

{#if mobile}
  <div class="settings-nav-mobile flex gap-1 overflow-x-auto pb-1">
    {#each SETTINGS_SECTIONS as section (section.id)}
      <button
        type="button"
        class="settings-nav-chip {active === section.id ? 'settings-nav-chip-active' : ''}"
        onclick={() => onSelect(section.id)}
      >
        {section.label}
      </button>
    {/each}
  </div>
{:else}
  <nav class="settings-nav" aria-label="Settings sections">
    {#each SETTINGS_SECTIONS as section (section.id)}
      <button
        type="button"
        class="settings-nav-item {active === section.id ? 'settings-nav-item-active' : ''}"
        aria-current={active === section.id ? "page" : undefined}
        onclick={() => onSelect(section.id)}
      >
        <span class="block text-sm font-medium">{section.label}</span>
        <span class="workshop-faint mt-0.5 block text-xs leading-snug">{section.hint}</span>
      </button>
    {/each}
  </nav>
{/if}
