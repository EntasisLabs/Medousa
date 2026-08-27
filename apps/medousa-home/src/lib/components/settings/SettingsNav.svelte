<script lang="ts">
  import {
    Blocks,
    Bot,
    Check,
    ChevronDown,
    Gauge,
    Link2,
    Package,
    RadioTower,
    Share2,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
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

  let pickerOpen = $state(false);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

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
  function openPicker() {
    haptic("light");
    pickerOpen = true;
  }

  function closePicker() {
    pickerOpen = false;
  }

  function selectSection(section: SettingsSectionId) {
    haptic("light");
    onSelect(section);
    closePicker();
  }

  $effect(() => {
    if (!pickerOpen) return;
    return registerMobileBackHandler(() => {
      closePicker();
      return true;
    });
  });

  $effect(() => {
    if (!pickerOpen || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, {
      onDismiss: closePicker,
      swipeBack: false,
    });
  });
</script>

{#if mobile}
  <button
    type="button"
    class="settings-nav-title-trigger"
    aria-label="Choose settings view"
    aria-haspopup="dialog"
    aria-expanded={pickerOpen}
    onclick={openPicker}
  >
    <span class="truncate">{activeLabel}</span>
    {#if activeBadge > 0}
      <span class="settings-nav-badge">{activeBadge}</span>
    {/if}
    <span class="settings-nav-title-chevron" aria-hidden="true">
      <ChevronDown size={15} strokeWidth={2} />
    </span>
  </button>

  {#if pickerOpen}
    <div
      class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) closePicker();
      }}
    >
      <div
        bind:this={sheetEl}
        class="mobile-sheet mobile-turn-sheet settings-nav-picker"
        role="dialog"
        aria-label="Choose settings view"
      >
        <header bind:this={headerEl} class="settings-nav-picker-header">
          <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
          <h2>Settings</h2>
        </header>
        <div class="mobile-turn-sheet-body">
          <div class="mobile-turn-sheet-group">
            {#each pagerSections as sectionId, index (sectionId)}
              {@const section = settingsSectionById(sectionId)}
              {#if section}
                <button
                  type="button"
                  class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                  aria-current={sectionId === active ? "page" : undefined}
                  onclick={() => selectSection(sectionId)}
                >
                  <span class="mobile-turn-sheet-row-copy">
                    <span class="mobile-turn-sheet-row-title">{section.label}</span>
                    <span class="mobile-turn-sheet-row-subtitle">{section.hint}</span>
                  </span>
                  {#if (badges[sectionId] ?? 0) > 0}
                    <span class="settings-nav-badge">{badges[sectionId]}</span>
                  {/if}
                  {#if sectionId === active}
                    <Check size={18} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                  {/if}
                </button>
              {/if}
            {/each}
          </div>
        </div>
      </div>
    </div>
  {/if}
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

  .settings-nav-title-trigger {
    display: inline-flex;
    min-width: 0;
    max-width: 13rem;
    align-items: center;
    justify-content: center;
    gap: 0.3rem;
    border: 0;
    background: transparent;
    padding: 0.55rem 0.7rem;
    color: rgb(var(--color-surface-50));
    font-size: 0.95rem;
    font-weight: 650;
    line-height: 1.2;
  }

  .settings-nav-title-trigger:active {
    opacity: 0.7;
  }

  .settings-nav-title-chevron {
    flex-shrink: 0;
    color: rgb(var(--theme-text-quiet));
    transition: transform 160ms ease;
  }

  .settings-nav-title-trigger[aria-expanded="true"] .settings-nav-title-chevron {
    transform: rotate(180deg);
  }

  .settings-nav-picker {
    max-height: min(82dvh, 42rem);
  }

  .settings-nav-picker-header {
    flex-shrink: 0;
    padding: 0 1rem 0.65rem;
    text-align: center;
  }

  .settings-nav-picker-header h2 {
    margin: 0.55rem 0 0;
    color: rgb(var(--color-surface-50));
    font-size: 0.95rem;
    font-weight: 650;
  }
</style>
