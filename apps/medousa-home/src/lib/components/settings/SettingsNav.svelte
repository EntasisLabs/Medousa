<script lang="ts">
  import type { SettingsSectionId } from "$lib/types/settings";
  import { SETTINGS_SECTIONS } from "$lib/types/settings";

  interface Props {
    active: SettingsSectionId;
    mobile?: boolean;
    badges?: Partial<Record<SettingsSectionId, number>>;
    onSelect: (section: SettingsSectionId) => void;
  }

  let { active, mobile = false, badges = {}, onSelect }: Props = $props();
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
        {#if (badges[section.id] ?? 0) > 0}
          <span class="settings-nav-badge">{badges[section.id]}</span>
        {/if}
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
        <span class="flex items-center gap-2 text-sm font-medium">
          {section.label}
          {#if (badges[section.id] ?? 0) > 0}
            <span class="settings-nav-badge">{badges[section.id]}</span>
          {/if}
        </span>
        <span class="workshop-faint mt-0.5 block text-xs leading-snug">{section.hint}</span>
      </button>
    {/each}
  </nav>
{/if}

<style>
  .settings-nav-badge {
    display: inline-flex;
    min-width: 1.1rem;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    padding: 0.05rem 0.35rem;
    font-size: 0.625rem;
    font-weight: 700;
    line-height: 1.2;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 40%, transparent);
  }
</style>
