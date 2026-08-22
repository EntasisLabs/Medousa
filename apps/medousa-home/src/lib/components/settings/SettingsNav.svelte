<script lang="ts">
  import {
    Blocks,
    Bot,
    ChevronLeft,
    ChevronRight,
    Gauge,
    Link2,
    Package,
    RadioTower,
    Share2,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import type { SettingsSectionId } from "$lib/types/settings";
  import {
    settingsMobileSections,
    settingsNavEntries,
    settingsSectionById,
  } from "$lib/types/settings";

  interface Props {
    active: SettingsSectionId;
    mobile?: boolean;
    variant?: "sidebar" | "rail";
    badges?: Partial<Record<SettingsSectionId, number>>;
    onSelect: (section: SettingsSectionId) => void;
  }

  let {
    active,
    mobile = false,
    variant = "sidebar",
    badges = {},
    onSelect,
  }: Props = $props();

  const entries = $derived(settingsNavEntries());
  const activeGroup = $derived(settingsSectionById(active)?.group ?? null);
  const activeLabel = $derived(settingsSectionById(active)?.label ?? "Settings");
  const activeBadge = $derived(badges[active] ?? 0);
  const rail = $derived(variant === "rail");

  const sectionIcons = {
    preferences: SlidersHorizontal,
    agent: Bot,
    runtime: Gauge,
    network: Share2,
    connections: Link2,
    packages: Package,
    mcp: Blocks,
    basement: RadioTower,
  } as const;

  const pagerSections = $derived(settingsMobileSections());
  const sectionIndex = $derived.by(() => {
    const idx = pagerSections.indexOf(active);
    return idx >= 0 ? idx : 0;
  });

  function stepSection(delta: number) {
    const len = pagerSections.length;
    const next = pagerSections[(sectionIndex + delta + len) % len]!;
    onSelect(next);
  }
</script>

{#if mobile}
  <div
    class="settings-nav-mobile-pager"
    role="navigation"
    aria-label="Settings sections"
  >
    <button
      type="button"
      class="settings-nav-pager-btn"
      title="Previous section"
      aria-label="Previous settings section"
      onclick={() => stepSection(-1)}
    >
      <ChevronLeft size={18} strokeWidth={2} />
    </button>
    <div class="settings-nav-pager-label" aria-live="polite">
      <span class="settings-nav-pager-title">
        {activeLabel}
        {#if activeBadge > 0}
          <span class="settings-nav-badge">{activeBadge}</span>
        {/if}
      </span>
      <span class="settings-nav-pager-dots" aria-hidden="true">
        {#each pagerSections as sectionId, i (sectionId)}
          <span
            class="settings-nav-pager-dot"
            class:settings-nav-pager-dot--active={i === sectionIndex}
          ></span>
        {/each}
      </span>
    </div>
    <button
      type="button"
      class="settings-nav-pager-btn"
      title="Next section"
      aria-label="Next settings section"
      onclick={() => stepSection(1)}
    >
      <ChevronRight size={18} strokeWidth={2} />
    </button>
  </div>
{:else}
  <nav
    class="settings-nav"
    class:settings-nav--rail={rail}
    aria-label="Settings sections"
  >
    {#each entries as entry (entry.kind === "group" ? `g-${entry.id}` : entry.section.id)}
      {#if entry.kind === "group"}
        <div
          class="settings-nav-group"
          class:settings-nav-group--active={entry.id === activeGroup}
        >
          {entry.label}
        </div>
      {:else}
        {@const Icon = sectionIcons[entry.section.id]}
        <button
          type="button"
          class="settings-nav-item {active === entry.section.id ? 'settings-nav-item-active' : ''}"
          aria-current={active === entry.section.id ? "page" : undefined}
          title={rail ? entry.section.hint : undefined}
          onclick={() => onSelect(entry.section.id)}
        >
          {#if rail}
            <Icon
              size={14}
              strokeWidth={1.75}
              class="settings-nav-item-icon"
              aria-hidden="true"
            />
          {/if}
          <span class="settings-nav-item-copy">
            <span class="settings-nav-item-label">{entry.section.label}</span>
            {#if (badges[entry.section.id] ?? 0) > 0}
              <span class="settings-nav-badge">{badges[entry.section.id]}</span>
            {/if}
          </span>
          {#if !rail}
            <span class="workshop-faint mt-0.5 block text-xs leading-snug">
              {entry.section.hint}
            </span>
          {/if}
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

  .settings-nav-mobile-pager {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .settings-nav-pager-btn {
    display: inline-flex;
    height: 2.25rem;
    width: 2.25rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 40%, transparent);
    color: rgb(var(--color-surface-200));
    background: transparent;
  }

  .settings-nav-pager-btn:active {
    background: color-mix(in srgb, var(--color-surface-500) 18%, transparent);
  }

  .settings-nav-pager-label {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0.35rem;
    text-align: center;
  }

  .settings-nav-pager-title {
    display: inline-flex;
    max-width: 100%;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.875rem;
    font-weight: 600;
    line-height: 1.2;
    color: rgb(var(--color-surface-50));
  }

  .settings-nav-pager-dots {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }

  .settings-nav-pager-dot {
    height: 0.28rem;
    width: 0.28rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-500) 55%, transparent);
  }

  .settings-nav-pager-dot--active {
    width: 0.75rem;
    background: color-mix(in srgb, var(--color-primary-500) 75%, transparent);
  }
</style>
