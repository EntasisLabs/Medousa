<script lang="ts">
  import type { SettingsSectionId } from "$lib/types/settings";
  import { settingsNavEntries, settingsSectionById } from "$lib/types/settings";

  interface Props {
    active: SettingsSectionId;
    mobile?: boolean;
    badges?: Partial<Record<SettingsSectionId, number>>;
    onSelect: (section: SettingsSectionId) => void;
  }

  let { active, mobile = false, badges = {}, onSelect }: Props = $props();

  const entries = $derived(settingsNavEntries());
  const activeGroup = $derived(settingsSectionById(active)?.group ?? null);
</script>

{#if mobile}
  <div class="settings-nav-mobile flex gap-1 overflow-x-auto pb-1">
    {#each entries as entry (entry.kind === "group" ? `g-${entry.id}` : entry.section.id)}
      {#if entry.kind === "group"}
        <span
          class="settings-nav-chip-group"
          class:settings-nav-chip-group--active={entry.id === activeGroup}
          aria-hidden="true"
        >
          {entry.label}
        </span>
      {:else}
        <button
          type="button"
          class="settings-nav-chip {active === entry.section.id ? 'settings-nav-chip-active' : ''}"
          onclick={() => onSelect(entry.section.id)}
        >
          {entry.section.label}
          {#if (badges[entry.section.id] ?? 0) > 0}
            <span class="settings-nav-badge">{badges[entry.section.id]}</span>
          {/if}
        </button>
      {/if}
    {/each}
  </div>
{:else}
  <nav class="settings-nav" aria-label="Settings sections">
    {#each entries as entry (entry.kind === "group" ? `g-${entry.id}` : entry.section.id)}
      {#if entry.kind === "group"}
        <div
          class="settings-nav-group"
          class:settings-nav-group--active={entry.id === activeGroup}
        >
          {entry.label}
        </div>
      {:else}
        <button
          type="button"
          class="settings-nav-item {active === entry.section.id ? 'settings-nav-item-active' : ''}"
          aria-current={active === entry.section.id ? "page" : undefined}
          onclick={() => onSelect(entry.section.id)}
        >
          <span class="flex items-center gap-2 text-sm font-medium">
            {entry.section.label}
            {#if (badges[entry.section.id] ?? 0) > 0}
              <span class="settings-nav-badge">{badges[entry.section.id]}</span>
            {/if}
          </span>
          <span class="workshop-faint mt-0.5 block text-xs leading-snug">{entry.section.hint}</span>
        </button>
      {/if}
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
