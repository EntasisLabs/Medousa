<script lang="ts">
  import {
    Activity,
    CalendarDays,
    Check,
    Code2,
    Compass,
    Globe,
    Home,
    LoaderCircle,
    MessageCircle,
    NotebookText,
    Plus,
    Radio,
    Settings,
    Sparkles,
    UserRound,
    Users,
    Zap,
  } from "@lucide/svelte";
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import { registerMobileBackHandler, switchMobileTab } from "$lib/mobileNavigation";
  import { environment } from "$lib/stores/environment.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { haptic } from "$lib/haptics";
  import {
    mobileEditableDestinationItems,
    mobileDestinationSections,
    settingsDestinationItem,
    type MobileDestinationItem,
  } from "$lib/utils/mobileDestinations";
  import {
    activeLayoutPreset,
    activePresetSurfaceIds,
    isNavDestinationToggleable,
  } from "$lib/utils/environmentLayout";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import type { Component } from "svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  let editing = $state(false);
  let editBusyId = $state<string | null>(null);
  let editError = $state<string | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  const icons: Record<string, Component> = {
    "tab-home": Home,
    "tab-chat": MessageCircle,
    "tab-notes": NotebookText,
    "more-code": Code2,
    "tab-web": Globe,
    "more-calendar": CalendarDays,
    "more-profiles": UserRound,
    "more-map": Compass,
    "more-workshop": Sparkles,
    "more-automations": Zap,
    "more-messaging": Radio,
    "more-peers": Users,
    "more-settings": Settings,
    "more-runtime": Activity,
  };

  const sections = $derived(mobileDestinationSections(environment.spec));
  const goToSection = $derived(sections.find((section) => section.title === "Go to"));
  const moreSection = $derived(sections.find((section) => section.title === "More"));
  const settingsItem = settingsDestinationItem();
  const visibleSurfaceIds = $derived(
    new Set(environment.spec ? activePresetSurfaceIds(environment.spec) : []),
  );
  const activeLayoutLabel = $derived(
    environment.spec
      ? (activeLayoutPreset(environment.spec)?.label ?? "Current layout")
      : "Current layout",
  );
  const editableItems = $derived(
    environment.spec ? mobileEditableDestinationItems(environment.spec) : [],
  );
  const customViews = $derived(
    environment.navSurfaces().filter((surface) => surface.kind === "custom"),
  );
  const editableCustomViews = $derived.by(() => {
    const spec = environment.spec;
    if (!spec) return [];
    const order = new Map(
      activePresetSurfaceIds(spec).map((surfaceId, index) => [surfaceId, index]),
    );
    return spec.surfaces
      .filter(
        (surface) =>
          surface.kind === "custom" && isNavDestinationToggleable(surface.id),
      )
      .map((surface, catalogIndex) => ({ surface, catalogIndex }))
      .sort((left, right) => {
        const leftIndex = order.get(left.surface.id) ?? Number.MAX_SAFE_INTEGER;
        const rightIndex = order.get(right.surface.id) ?? Number.MAX_SAFE_INTEGER;
        return leftIndex - rightIndex || left.catalogIndex - right.catalogIndex;
      })
      .map(({ surface }) => surface);
  });

  function pick(item: MobileDestinationItem) {
    haptic("light");
    onClose();
    if (item.kind === "tab" && item.tab) {
      switchMobileTab(item.tab);
      return;
    }
    if (item.kind === "more" && item.more) {
      layout.openMore(item.more);
    }
  }

  function pickCustom(surfaceId: string) {
    haptic("light");
    onClose();
    layout.openCustomSurface(surfaceId);
  }

  function isActive(item: MobileDestinationItem): boolean {
    if (item.kind === "tab" && item.tab) {
      if (item.tab === "home") {
        return layout.mobileTab === "home" && !layout.mobileSurfaceOverride;
      }
      return layout.mobileTab === item.tab;
    }
    return (
      item.kind === "more" &&
      Boolean(item.more) &&
      layout.mobileTab === "more" &&
      layout.moreDestination === item.more
    );
  }

  function isCustomViewActive(surfaceId: string): boolean {
    return (
      layout.mobileTab === "home" && layout.mobileSurfaceOverride === surfaceId
    );
  }

  function finishOrClose() {
    if (editing) {
      editing = false;
      editError = null;
      return;
    }
    onClose();
  }

  async function toggleSurface(surfaceId: string) {
    if (!environment.spec || editBusyId) return;
    haptic("light");
    editBusyId = surfaceId;
    editError = null;
    try {
      await environment.setSurfaceNavVisible(
        surfaceId,
        !visibleSurfaceIds.has(surfaceId),
      );
    } catch (err) {
      editError = err instanceof Error ? err.message : String(err);
    } finally {
      editBusyId = null;
    }
  }

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      onClose();
      return true;
    });
  });

  $effect(() => {
    if (!open || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, {
      onDismiss: onClose,
      swipeBack: false,
    });
  });

  /** Native WKWebView paints above DOM — hide the embed while the menu is open. */
  $effect(() => {
    if (!open) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") onClose();
    }}
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={sheetEl}
      class="mobile-sheet mobile-turn-sheet mobile-sheet-tall mobile-dest-menu"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Destinations"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
      <header bind:this={headerEl} class="mobile-dest-menu-head">
        {#if editing}
          <span class="mobile-dest-menu-header-spacer" aria-hidden="true"></span>
        {:else}
          <button
            type="button"
            class="mobile-dest-menu-edit-toggle"
            disabled={!environment.spec || editBusyId !== null}
            onclick={() => {
              haptic("light");
              editing = !editing;
              editError = null;
            }}
          >
            {editing ? "Done" : "Edit"}
          </button>
        {/if}
        <h2 class="mobile-dest-menu-title">{editing ? "Edit menu" : "Menu"}</h2>
        <button type="button" class="mobile-dest-menu-done" onclick={finishOrClose}>Done</button>
      </header>

      {#if !editing && userProfiles.hasMultipleProfiles}
        <div class="mobile-dest-menu-switchers">
          <ProfileSwitcherCompact />
        </div>
      {/if}

      <div class="mobile-turn-sheet-body mobile-dest-menu-scroll">
        {#if editing}
          <div class="mobile-dest-menu-edit-intro">
            <strong>{activeLayoutLabel}</strong>
            <span>
              Choose what this layout shows in mobile navigation. Changes also
              apply to its desktop rail.
            </span>
          </div>

          <p class="mobile-dest-menu-section">Destinations</p>
          <ul class="mobile-turn-sheet-group mobile-dest-menu-list">
            {#each editableItems as item, index (item.id)}
              {@const Icon = icons[item.id] ?? Sparkles}
              {#if item.surfaceId}
                {@const visible = visibleSurfaceIds.has(item.surfaceId)}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row mobile-dest-menu-row mobile-dest-menu-edit-row {index > 0
                      ? 'mobile-turn-sheet-row-divider'
                      : ''}"
                    class:mobile-dest-menu-edit-row-hidden={!visible}
                    aria-pressed={visible}
                    aria-label={`${visible ? "Hide" : "Show"} ${item.label}`}
                    disabled={editBusyId !== null}
                    onclick={() => void toggleSurface(item.surfaceId!)}
                  >
                    <span class="mobile-dest-menu-icon" aria-hidden="true">
                      <Icon size={19} strokeWidth={1.75} />
                    </span>
                    <span class="mobile-turn-sheet-row-title min-w-0 flex-1 text-left">
                      {item.label}
                    </span>
                    <span
                      class="mobile-dest-menu-edit-state"
                      class:mobile-dest-menu-edit-state-active={visible}
                      aria-hidden="true"
                    >
                      {#if editBusyId === item.surfaceId}
                        <LoaderCircle size={14} strokeWidth={2} class="animate-spin" />
                      {:else if visible}
                        <Check size={14} strokeWidth={2.25} />
                      {:else}
                        <Plus size={14} strokeWidth={2} />
                      {/if}
                    </span>
                  </button>
                </li>
              {/if}
            {/each}
          </ul>

          {#if editableCustomViews.length > 0}
            <p class="mobile-dest-menu-section">My views</p>
            <ul class="mobile-turn-sheet-group mobile-dest-menu-list">
              {#each editableCustomViews as view, index (view.id)}
                {@const visible = visibleSurfaceIds.has(view.id)}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row mobile-dest-menu-row mobile-dest-menu-edit-row {index > 0
                      ? 'mobile-turn-sheet-row-divider'
                      : ''}"
                    class:mobile-dest-menu-edit-row-hidden={!visible}
                    aria-pressed={visible}
                    aria-label={`${visible ? "Hide" : "Show"} ${view.label}`}
                    disabled={editBusyId !== null}
                    onclick={() => void toggleSurface(view.id)}
                  >
                    <span class="mobile-dest-menu-icon" aria-hidden="true">
                      <Sparkles size={19} strokeWidth={1.75} />
                    </span>
                    <span class="mobile-turn-sheet-row-title min-w-0 flex-1 text-left">
                      {view.label}
                    </span>
                    <span
                      class="mobile-dest-menu-edit-state"
                      class:mobile-dest-menu-edit-state-active={visible}
                      aria-hidden="true"
                    >
                      {#if editBusyId === view.id}
                        <LoaderCircle size={14} strokeWidth={2} class="animate-spin" />
                      {:else if visible}
                        <Check size={14} strokeWidth={2.25} />
                      {:else}
                        <Plus size={14} strokeWidth={2} />
                      {/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if editError}
            <p class="mobile-dest-menu-edit-error" role="alert">{editError}</p>
          {/if}
        {:else}
          {#if goToSection}
            <p class="mobile-dest-menu-section">{goToSection.title}</p>
            <ul class="mobile-turn-sheet-group mobile-dest-menu-list">
              {#each goToSection.items as item, index (item.id)}
                {@const Icon = icons[item.id] ?? Sparkles}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row mobile-dest-menu-row {index > 0
                      ? 'mobile-turn-sheet-row-divider'
                      : ''}"
                    aria-current={isActive(item) ? "page" : undefined}
                    onclick={() => pick(item)}
                  >
                    <span class="mobile-dest-menu-icon" aria-hidden="true">
                      <Icon size={19} strokeWidth={1.75} />
                    </span>
                    <span class="mobile-turn-sheet-row-copy text-left">
                      <span class="mobile-turn-sheet-row-title">{item.label}</span>
                      {#if item.hint}
                        <span class="mobile-turn-sheet-row-subtitle line-clamp-2">{item.hint}</span>
                      {/if}
                    </span>
                    {#if isActive(item)}
                      <Check size={18} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if customViews.length > 0}
            <p class="mobile-dest-menu-section">My views</p>
            <ul class="mobile-turn-sheet-group mobile-dest-menu-list">
              {#each customViews as view, index (view.id)}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row mobile-dest-menu-row {index > 0
                      ? 'mobile-turn-sheet-row-divider'
                      : ''}"
                    aria-current={isCustomViewActive(view.id) ? "page" : undefined}
                    onclick={() => pickCustom(view.id)}
                  >
                    <span class="mobile-dest-menu-icon" aria-hidden="true">
                      <Sparkles size={19} strokeWidth={1.75} />
                    </span>
                    <span class="mobile-turn-sheet-row-copy text-left">
                      <span class="mobile-turn-sheet-row-title">{view.label}</span>
                    </span>
                    {#if isCustomViewActive(view.id)}
                      <Check size={18} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if moreSection}
            <p class="mobile-dest-menu-section">{moreSection.title}</p>
            <ul class="mobile-turn-sheet-group mobile-dest-menu-list">
              {#each moreSection.items as item, index (item.id)}
                {@const Icon = icons[item.id] ?? Sparkles}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row mobile-dest-menu-row {index > 0
                      ? 'mobile-turn-sheet-row-divider'
                      : ''}"
                    aria-current={isActive(item) ? "page" : undefined}
                    onclick={() => pick(item)}
                  >
                    <span class="mobile-dest-menu-icon" aria-hidden="true">
                      <Icon size={19} strokeWidth={1.75} />
                    </span>
                    <span class="mobile-turn-sheet-row-copy text-left">
                      <span class="mobile-turn-sheet-row-title">{item.label}</span>
                      {#if item.hint}
                        <span class="mobile-turn-sheet-row-subtitle line-clamp-2">{item.hint}</span>
                      {/if}
                    </span>
                    {#if isActive(item)}
                      <Check size={18} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          <ul class="mobile-turn-sheet-group mobile-dest-menu-list mobile-dest-menu-list--tail">
            <li>
              <button
                type="button"
                class="mobile-turn-sheet-row mobile-dest-menu-row"
                aria-current={isActive(settingsItem) ? "page" : undefined}
                onclick={() => pick(settingsItem)}
              >
                <span class="mobile-dest-menu-icon" aria-hidden="true">
                  <Settings size={19} strokeWidth={1.75} />
                </span>
                <span class="mobile-turn-sheet-row-copy text-left">
                  <span class="mobile-turn-sheet-row-title">
                    {settingsItem.label}
                  </span>
                  {#if settingsItem.hint}
                    <span class="mobile-turn-sheet-row-subtitle line-clamp-2">
                      {settingsItem.hint}
                    </span>
                  {/if}
                </span>
                {#if isActive(settingsItem)}
                  <Check size={18} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                {/if}
              </button>
            </li>
          </ul>
        {/if}
      </div>
    </div>
  </div>
{/if}
