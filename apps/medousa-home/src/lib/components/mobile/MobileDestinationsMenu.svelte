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
    X,
    Zap,
  } from "@lucide/svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import { registerMobileBackHandler, switchMobileTab } from "$lib/mobileNavigation";
  import { environment } from "$lib/stores/environment.svelte";
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
  import type { Component } from "svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    onToggleActivity?: () => void;
  }

  let { open, onClose, onToggleActivity }: Props = $props();

  let editing = $state(false);
  let editBusyId = $state<string | null>(null);
  let editError = $state<string | null>(null);

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
    class="mobile-dest-menu-backdrop"
    role="presentation"
    onclick={onClose}
    onkeydown={(e) => {
      if (e.key === "Escape") onClose();
    }}
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="mobile-dest-menu"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Destinations"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <header class="mobile-dest-menu-head">
        <h2 class="mobile-dest-menu-title">{editing ? "Edit menu" : "Menu"}</h2>
        <div class="mobile-dest-menu-head-actions">
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
          <button type="button" class="mobile-icon-btn" aria-label="Close" onclick={onClose}>
            <X size={18} strokeWidth={1.75} />
          </button>
        </div>
      </header>

      {#if !editing}
        <div class="mobile-dest-menu-switchers">
          <WorkshopSwitcherCompact />
          <ProfileSwitcherCompact />
          {#if onToggleActivity}
            <button
              type="button"
              class="mobile-dest-menu-activity"
              onclick={() => {
                onClose();
                onToggleActivity();
              }}
            >
              Activity
            </button>
          {/if}
        </div>
      {/if}

      <div class="mobile-dest-menu-scroll">
        {#if editing}
          <div class="mobile-dest-menu-edit-intro">
            <strong>{activeLayoutLabel}</strong>
            <span>
              Choose what this layout shows in mobile navigation. Changes also
              apply to its desktop rail.
            </span>
          </div>

          <p class="mobile-dest-menu-section">Destinations</p>
          <ul class="mobile-dest-menu-list">
            {#each editableItems as item (item.id)}
              {@const Icon = icons[item.id] ?? Sparkles}
              {#if item.surfaceId}
                {@const visible = visibleSurfaceIds.has(item.surfaceId)}
                <li>
                  <button
                    type="button"
                    class="mobile-dest-menu-row mobile-dest-menu-edit-row"
                    class:mobile-dest-menu-edit-row-hidden={!visible}
                    aria-pressed={visible}
                    aria-label={`${visible ? "Hide" : "Show"} ${item.label}`}
                    disabled={editBusyId !== null}
                    onclick={() => void toggleSurface(item.surfaceId!)}
                  >
                    <Icon size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                    <span class="mobile-dest-menu-row-title min-w-0 flex-1 text-left">
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
            <ul class="mobile-dest-menu-list">
              {#each editableCustomViews as view (view.id)}
                {@const visible = visibleSurfaceIds.has(view.id)}
                <li>
                  <button
                    type="button"
                    class="mobile-dest-menu-row mobile-dest-menu-edit-row"
                    class:mobile-dest-menu-edit-row-hidden={!visible}
                    aria-pressed={visible}
                    aria-label={`${visible ? "Hide" : "Show"} ${view.label}`}
                    disabled={editBusyId !== null}
                    onclick={() => void toggleSurface(view.id)}
                  >
                    <Sparkles size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                    <span class="mobile-dest-menu-row-title min-w-0 flex-1 text-left">
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
            <ul class="mobile-dest-menu-list">
              {#each goToSection.items as item (item.id)}
                {@const Icon = icons[item.id] ?? Sparkles}
                <li>
                  <button type="button" class="mobile-dest-menu-row" onclick={() => pick(item)}>
                    <Icon size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                    <span class="min-w-0 flex-1 text-left">
                      <span class="mobile-dest-menu-row-title">{item.label}</span>
                      {#if item.hint}
                        <span class="mobile-dest-menu-row-hint line-clamp-2">{item.hint}</span>
                      {/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if customViews.length > 0}
            <p class="mobile-dest-menu-section">My views</p>
            <ul class="mobile-dest-menu-list">
              {#each customViews as view (view.id)}
                <li>
                  <button
                    type="button"
                    class="mobile-dest-menu-row"
                    onclick={() => pickCustom(view.id)}
                  >
                    <Sparkles size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                    <span class="mobile-dest-menu-row-title">{view.label}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if moreSection}
            <p class="mobile-dest-menu-section">{moreSection.title}</p>
            <ul class="mobile-dest-menu-list">
              {#each moreSection.items as item (item.id)}
                {@const Icon = icons[item.id] ?? Sparkles}
                <li>
                  <button type="button" class="mobile-dest-menu-row" onclick={() => pick(item)}>
                    <Icon size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                    <span class="min-w-0 flex-1 text-left">
                      <span class="mobile-dest-menu-row-title">{item.label}</span>
                      {#if item.hint}
                        <span class="mobile-dest-menu-row-hint line-clamp-2">{item.hint}</span>
                      {/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          <ul class="mobile-dest-menu-list mobile-dest-menu-list--tail">
            <li>
              <button
                type="button"
                class="mobile-dest-menu-row"
                onclick={() => pick(settingsItem)}
              >
                <Settings size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                <span class="min-w-0 flex-1 text-left">
                  <span class="mobile-dest-menu-row-title">
                    {settingsItem.label}
                  </span>
                  {#if settingsItem.hint}
                    <span class="mobile-dest-menu-row-hint line-clamp-2">
                      {settingsItem.hint}
                    </span>
                  {/if}
                </span>
              </button>
            </li>
          </ul>
        {/if}
      </div>
    </div>
  </div>
{/if}
