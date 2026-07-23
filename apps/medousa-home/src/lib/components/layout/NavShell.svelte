<script lang="ts">
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import ContextSidePanel from "$lib/components/context/ContextSidePanel.svelte";
  import SessionRailToolbar from "$lib/components/chat/SessionRailToolbar.svelte";
  import ContextModeBar from "$lib/components/context/ContextModeBar.svelte";
  import NavRailViewPopover from "$lib/components/layout/NavRailViewPopover.svelte";
  import LmeSidePanel from "$lib/components/lme/LmeSidePanel.svelte";
  import MessagingChannelList from "$lib/components/messaging/MessagingChannelList.svelte";
  import MessagingRailToolbar from "$lib/components/messaging/MessagingRailToolbar.svelte";
  import PeersRailToolbar from "$lib/components/peers/PeersRailToolbar.svelte";
  import PeersShellList from "$lib/components/peers/PeersShellList.svelte";
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace, type LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";
  import { messaging } from "$lib/stores/messaging.svelte";
  import { messagingShell } from "$lib/stores/messagingShell.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { feedBadgeForComponents } from "$lib/utils/customViewStatus";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    defaultModeForLmeFamily,
    isLmeAutomationsMode,
    isLmeLibraryMode,
    labelForLmeExplorerMode,
    type LmeExplorerFamily,
  } from "$lib/utils/lmeExplorerModes";
  import {
    automationsRailSurface,
    buildLifeRailLayout,
    libraryRailSurface,
  } from "$lib/utils/lifeRailSections";
  import type { LifeRailItem } from "$lib/utils/lifeRailItems";
  import {
    navLabel,
    navTitle,
    shellSidebarViewTitle,
    surfaceHasShellSidebarView,
  } from "$lib/utils/navSurfaces";
  import {
    activateNestItem,
    NAV_RAIL_NEST_LIMIT,
    nestItemIsActive,
    nestItemsForSurface,
    prefetchRailNestData,
    surfaceSupportsRailNest,
    type NavRailNestItem,
  } from "$lib/utils/navRailNest";
  import {
    registerRailPopoverSummon,
    setLastPointer,
    type RailPopoverCursor,
  } from "$lib/utils/railPopoverSummon";
  import { resolveSummonToolbarSurface } from "$lib/utils/resolveSummonToolbarSurface";
  import { toast } from "$lib/stores/toast.svelte";
  import {
    ChevronLeft,
    ChevronRight,
    PanelLeftClose,
    Settings,
  } from "@lucide/svelte";
  import { SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";
  import type { DaemonHealth } from "$lib/daemon";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { onMount } from "svelte";

  type RailPopoverTarget =
    | { kind: "lme"; mode: LmeExplorerMode }
    | { kind: "surface"; surfaceId: string };

  const NEST_OPEN_KEY = "medousa-home-rail-nest-open";

  function loadNestOpen(): Record<string, boolean> {
    if (typeof localStorage === "undefined") return {};
    try {
      const raw = localStorage.getItem(NEST_OPEN_KEY);
      if (!raw) return {};
      const parsed = JSON.parse(raw) as Record<string, boolean>;
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  }


  interface Props {
    active: string;
    onSelect: (surface: string) => void;
    onOpenChat: () => void;
    health?: DaemonHealth | null;
    chatActivity?: number;
    workActivity?: number;
    peersActivity?: number;
    activeProfileLabel?: string;
  }

  let {
    active,
    onSelect,
    onOpenChat,
    health = null,
    chatActivity = 0,
    workActivity = 0,
    peersActivity = 0,
    activeProfileLabel = "Personal",
  }: Props = $props();

  const mode = $derived(layout.shellSidebarMode);
  /** List hosted in the rail — may differ from main `active` content surface. */
  const viewSurface = $derived(layout.shellSidebarViewSurface ?? active);
  /** View list fills the same rail — never a second column. */
  const showView = $derived(
    mode === "view" && surfaceHasShellSidebarView(viewSurface),
  );
  const viewTitle = $derived(
    viewSurface === "library" || viewSurface === "automations"
      ? labelForLmeExplorerMode(lmeWorkspace.explorerMode)
      : shellSidebarViewTitle(viewSurface),
  );
  const daemonOk = $derived(health?.ok ?? false);

  const surfaces = $derived(environment.navSurfaces());
  const lifeRail = $derived(buildLifeRailLayout(surfaces));

  /** Quieter tree parents — closer to Cursor folder icons. */
  const treeIconProps = { size: 14, strokeWidth: 1.5 };
  const heroIconProps = { size: 17, strokeWidth: 1.85 };
  const utilityIconProps = { size: 14, strokeWidth: 1.5 };
  /** Explicit open map; missing key = collapsed by default. */
  let nestOpen = $state<Record<string, boolean>>(loadNestOpen());
  /** In-place flyout — replaces rail view-mode swaps for list surfaces. */
  let railPopover = $state<RailPopoverTarget | null>(null);
  let railPopoverTriggerEl = $state<HTMLElement | null>(null);
  /** Click point so the toolbar floats next to the mouse (not rail-docked). */
  let railPopoverCursor = $state<{ x: number; y: number } | null>(null);
  /** Landing phase — summon uses toolbar; rail clicks use seed. */
  let railPopoverPreferPhase = $state<"seed" | "toolbar">("seed");
  /** Invisible 1×1 anchor when the rail button isn’t in the DOM. */
  let syntheticTriggerEl: HTMLElement | null = null;

  const railPopoverTitle = $derived(
    railPopover?.kind === "lme"
      ? labelForLmeExplorerMode(railPopover.mode)
      : railPopover
        ? shellSidebarViewTitle(railPopover.surfaceId)
        : "",
  );
  const railPopoverOpen = $derived(railPopover !== null);
  const railPopoverTargetKey = $derived(
    railPopover?.kind === "lme"
      ? `lme:${railPopover.mode}`
      : railPopover
        ? `surface:${railPopover.surfaceId}`
        : "",
  );
  const railPopoverUsesLmeDock = $derived(
    railPopover?.kind === "lme" ||
      railPopover?.surfaceId === "library" ||
      railPopover?.surfaceId === "automations",
  );

  function activityFor(id: string): number {
    if (id === "chat") return chatActivity;
    if (id === "work") return workActivity;
    if (id === "peers") return peersActivity;
    return 0;
  }

  function showCountBadge(id: string): boolean {
    return id === "peers";
  }

  function feedBadgeForSurface(surface: (typeof surfaces)[number]): "live" | "stale" | "none" {
    if (surface.kind !== "custom") return "none";
    return feedBadgeForComponents(
      environment.componentsForSurface(surface.id),
      environment.feedStateByComponentId,
    );
  }

  function railBtnClass(
    id: string,
    tier: "life" | "utility",
    options?: { quietActive?: boolean; active?: boolean },
  ): string {
    const isActive = options?.active ?? active === id;
    const activeClass = isActive
      ? options?.quietActive
        ? "workshop-rail-btn-active-quiet"
        : "workshop-rail-btn-active"
      : "";
    const tierClass =
      tier === "life" ? "workshop-rail-btn-tier-life" : "workshop-rail-btn-tier-utility";
    return `workshop-rail-btn relative ${tierClass} ${activeClass}`;
  }

  function nestHasActiveItem(surfaceId: string, nest: NavRailNestItem[]): boolean {
    return nest.some((item) => nestItemIsActive(surfaceId, item.id));
  }

  function libraryIsActive(): boolean {
    if (surfacePopoverOpen("library")) return true;
    if (railPopover?.kind === "lme" && isLmeLibraryMode(railPopover.mode)) return true;
    if (showView && viewSurface === "library") return true;
    if (active !== "library" && active !== "automations") return false;
    return isLmeLibraryMode(lmeWorkspace.explorerMode);
  }

  function automationsIsActive(): boolean {
    if (surfacePopoverOpen("automations")) return true;
    if (railPopover?.kind === "lme" && isLmeAutomationsMode(railPopover.mode)) return true;
    if (showView && viewSurface === "automations") return true;
    if (active !== "library" && active !== "automations") return false;
    return isLmeAutomationsMode(lmeWorkspace.explorerMode);
  }

  function ensureFamilyForSurface(surfaceId: string) {
    if (surfaceId === "library" && !isLmeLibraryMode(lmeWorkspace.explorerMode)) {
      lmeWorkspace.setExplorerMode(defaultModeForLmeFamily("library"));
    } else if (
      surfaceId === "automations" &&
      !isLmeAutomationsMode(lmeWorkspace.explorerMode)
    ) {
      lmeWorkspace.setExplorerMode(defaultModeForLmeFamily("automations"));
    }
  }

  function lmeFamilyForSurface(surfaceId: string): LmeExplorerFamily {
    return surfaceId === "automations" ? "automations" : "library";
  }

  const YOU_NEST_KEY = "you";

  function youNestItems(): NavRailNestItem[] {
    if (!lifeRail.context || lifeRail.context.kind !== "surface") return [];
    return [
      {
        id: "context",
        label: navLabel(lifeRail.context.surface),
      },
    ];
  }

  function surfacePopoverOpen(surfaceId: string): boolean {
    return railPopover?.kind === "surface" && railPopover.surfaceId === surfaceId;
  }

  function disposeSyntheticTrigger() {
    if (!syntheticTriggerEl) return;
    syntheticTriggerEl.remove();
    syntheticTriggerEl = null;
  }

  function ensureSyntheticTrigger(cursor: RailPopoverCursor): HTMLElement {
    disposeSyntheticTrigger();
    const el = document.createElement("div");
    el.setAttribute("data-rail-popover-synthetic-trigger", "");
    el.setAttribute("aria-hidden", "true");
    el.style.cssText = `position:fixed;left:${cursor.x}px;top:${cursor.y}px;width:1px;height:1px;pointer-events:none;opacity:0;z-index:-1;`;
    document.body.appendChild(el);
    syntheticTriggerEl = el;
    return el;
  }

  function findRailTrigger(surfaceId: string): HTMLElement | null {
    if (typeof document === "undefined") return null;
    return document.querySelector(`[data-rail-surface="${CSS.escape(surfaceId)}"]`);
  }

  function closeRailPopover() {
    railPopover = null;
    railPopoverTriggerEl = null;
    railPopoverCursor = null;
    railPopoverPreferPhase = "seed";
    disposeSyntheticTrigger();
  }

  function sameRailPopover(target: RailPopoverTarget): boolean {
    if (!railPopover) return false;
    if (target.kind === "lme" && railPopover.kind === "lme") {
      return railPopover.mode === target.mode;
    }
    if (target.kind === "surface" && railPopover.kind === "surface") {
      return railPopover.surfaceId === target.surfaceId;
    }
    return false;
  }

  function openRailPopover(
    target: RailPopoverTarget,
    trigger: HTMLElement,
    event?: MouseEvent,
    options?: { cursor?: RailPopoverCursor; preferPhase?: "seed" | "toolbar" },
  ) {
    if (sameRailPopover(target)) {
      closeRailPopover();
      return;
    }
    railPopoverPreferPhase = options?.preferPhase ?? "seed";
    railPopoverTriggerEl = trigger;
    if (options?.cursor) {
      railPopoverCursor = options.cursor;
    } else if (event) {
      railPopoverCursor = { x: event.clientX, y: event.clientY };
    } else {
      const rect = trigger.getBoundingClientRect();
      railPopoverCursor = {
        x: rect.left + rect.width / 2,
        y: rect.top + rect.height / 2,
      };
    }
    railPopover = target;
  }

  /** Hotkey / mouse-shake: compact toolbar for the current view at the cursor. */
  function handleSummonViewToolbar(cursor?: RailPopoverCursor | null): boolean {
    const surfaceId = resolveSummonToolbarSurface(
      layout.desktopSurface,
      lmeWorkspace.explorerMode,
    );
    if (!surfaceId) {
      toast.show("No toolbar for this view", { durationMs: 1400 });
      return true;
    }

    const point: RailPopoverCursor =
      cursor ??
      ({
        x: typeof window !== "undefined" ? window.innerWidth / 2 : 0,
        y: typeof window !== "undefined" ? window.innerHeight / 2 : 0,
      });

    const target: RailPopoverTarget = { kind: "surface", surfaceId };
    if (sameRailPopover(target)) {
      closeRailPopover();
      return true;
    }

    ensureFamilyForSurface(surfaceId);
    const trigger = findRailTrigger(surfaceId) ?? ensureSyntheticTrigger(point);
    openRailPopover(target, trigger, undefined, {
      cursor: point,
      preferPhase: "toolbar",
    });
    return true;
  }

  function hideRail() {
    closeRailPopover();
    layout.setShellSidebarExpanded(false);
    void environment.patchShellChromeDesktop({ navStyle: "compact" }).catch(() => {});
  }

  function selectDestination(surfaceId: string, event?: MouseEvent) {
    if (surfaceHasShellSidebarView(surfaceId) && event) {
      event.preventDefault();
      event.stopPropagation();
      // Popover only — don't open a shell tab until the user picks something.
      ensureFamilyForSurface(surfaceId);
      layout.setShellSidebarMode("nav");
      openRailPopover(
        { kind: "surface", surfaceId },
        event.currentTarget as HTMLElement,
        event,
      );
      return;
    }
    closeRailPopover();
    ensureFamilyForSurface(surfaceId);
    onSelect(surfaceId);
    layout.setShellSidebarMode("nav");
  }

  /** Open the real surface/tab after a concrete pick inside a rail popover. */
  function commitPopoverSurface(surfaceId: string) {
    closeRailPopover();
    onSelect(surfaceId);
    layout.setShellSidebarMode("nav");
  }

  /** Popover → full side-rail view only (no main-content / tab activation). */
  function dockPopoverToRail() {
    if (!railPopover) return;
    if (railPopover.kind === "lme") {
      const mode = railPopover.mode;
      lmeWorkspace.setExplorerMode(mode);
      closeRailPopover();
      layout.openShellSidebarView(isLmeAutomationsMode(mode) ? "automations" : "library");
      return;
    }
    const surfaceId = railPopover.surfaceId;
    ensureFamilyForSurface(surfaceId);
    closeRailPopover();
    layout.openShellSidebarView(surfaceId);
  }

  function backToNav() {
    closeRailPopover();
    layout.shellSidebarBackToNav();
  }

  function nestFor(surfaceId: string): NavRailNestItem[] {
    if (!surfaceSupportsRailNest(surfaceId)) return [];
    return nestItemsForSurface(surfaceId);
  }

  function isNestExpanded(nestKey: string): boolean {
    return nestOpen[nestKey] === true;
  }

  function persistNestOpen() {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(NEST_OPEN_KEY, JSON.stringify(nestOpen));
  }

  function toggleNest(nestKey: string, event: Event) {
    event.preventDefault();
    event.stopPropagation();
    nestOpen = { ...nestOpen, [nestKey]: !isNestExpanded(nestKey) };
    persistNestOpen();
  }


  async function openNestItem(surfaceId: string, item: NavRailNestItem) {
    // Do not call onSelect(surface) — for chat that re-opens the *current* session
    // and races ensureSessionHydrated against the nest target (transcript bleed).
    closeRailPopover();
    layout.setShellSidebarMode("nav");
    if (!isNestExpanded(surfaceId)) {
      nestOpen = { ...nestOpen, [surfaceId]: true };
      persistNestOpen();
    }
    await activateNestItem(surfaceId, item.id);
  }

  onMount(() => {
    prefetchRailNestData();
    registerRailPopoverSummon(handleSummonViewToolbar);
    const onPointerMove = (event: PointerEvent) => {
      setLastPointer({ x: event.clientX, y: event.clientY });
    };
    window.addEventListener("pointermove", onPointerMove, { passive: true });
    return () => {
      registerRailPopoverSummon(null);
      window.removeEventListener("pointermove", onPointerMove);
      disposeSyntheticTrigger();
    };
  });
</script>

<nav
  class="workshop-icon-rail workshop-icon-rail-expanded master-rail-root {showView
    ? 'workshop-icon-rail-view'
    : ''}"
  aria-label={showView ? viewTitle : "Primary navigation"}
  data-debug-label="nav-master-rail"
  data-rail-mode={showView ? "view" : "nav"}
>
  {#key showView ? "view" : "nav"}
    <div
      class="master-rail-mode"
      in:fly={{ x: showView ? 10 : -10, duration: 180, opacity: 0, easing: cubicOut }}
      out:fade={{ duration: 110 }}
    >
      {#if showView}
        <header class="master-rail-header">
          <button
            type="button"
            class="master-rail-back"
            title="Back to navigation"
            aria-label="Back to navigation"
            onclick={backToNav}
          >
            <ChevronLeft size={16} strokeWidth={1.75} />
            <span>Back</span>
          </button>
          <p class="master-rail-view-title">{viewTitle}</p>
          <button
            type="button"
            class="master-rail-collapse"
            title="Hide rail"
            aria-label="Hide rail"
            onclick={hideRail}
          >
            <PanelLeftClose size={15} strokeWidth={1.75} />
          </button>
        </header>

        <div class="master-rail-view-body">
          {#if viewSurface === SAFETY_SURFACE_SETTINGS}
            <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1">
              <SettingsNav
                active={settingsNav.activeSection}
                onSelect={(section) => settingsNav.setActiveSection(section)}
              />
            </div>
          {:else if viewSurface === "chat"}
            <SessionSidebar open={true} variant="inline" />
          {:else if viewSurface === "library" || viewSurface === "automations"}
            <LmeSidePanel {onOpenChat} family={lmeFamilyForSurface(viewSurface)} />
          {:else if viewSurface === "messaging"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <label class="block px-1.5 pb-1.5 pt-1">
                <span class="sr-only">Search channels</span>
                <input
                  class="input w-full text-sm"
                  type="search"
                  placeholder="Search channels…"
                  value={messagingShell.search}
                  oninput={(event) => {
                    messagingShell.search = (event.currentTarget as HTMLInputElement).value;
                  }}
                />
              </label>
              <div class="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
                <MessagingChannelList
                  search={messagingShell.search}
                  selected={messagingShell.selectedChannel}
                  summary={messaging.summary}
                  {daemonOk}
                  loading={messaging.loading}
                  error={messaging.error}
                  onSelect={(id) => messagingShell.selectChannel(id)}
                />
              </div>
            </div>
          {:else if viewSurface === "peers"}
            <PeersShellList />
          {:else if viewSurface === "context"}
            <ContextSidePanel />
          {/if}
        </div>
      {:else}
        <header class="master-rail-header master-rail-header-nav">
          <span class="master-rail-nav-spacer" aria-hidden="true"></span>
          <button
            type="button"
            class="master-rail-collapse"
            title="Hide rail"
            aria-label="Hide rail"
            onclick={hideRail}
          >
            <PanelLeftClose size={15} strokeWidth={1.75} />
          </button>
        </header>

        <div
          class="workshop-icon-rail-items workshop-rail-tree workshop-rail-tree-jobs flex min-h-0 flex-1 flex-col overflow-y-auto"
        >
          {#snippet railDest(item: LifeRailItem, hero = false)}
            {#if item.kind === "surface"}
              {@const surface = item.surface}
              {@const Icon = environmentIcon(surface.icon)}
              {@const badge = activityFor(surface.id)}
              {@const feedBadge = feedBadgeForSurface(surface)}
              {@const nest = nestFor(surface.id)}
              {@const leafActive = nestHasActiveItem(surface.id, nest)}
              {@const nestExpanded = nest.length > 0 && isNestExpanded(surface.id)}
              {@const isLibrary = surface.id === "library"}
              {@const isAutomations = surface.id === "automations"}
              {@const doorActive = isLibrary
                ? libraryIsActive()
                : isAutomations
                  ? automationsIsActive()
                  : active === surface.id || surfacePopoverOpen(surface.id)}
              <div
                class="workshop-rail-dest"
                class:workshop-rail-dest-hero={hero}
                class:workshop-rail-dest-has-nest={nest.length > 0}
                class:workshop-rail-dest-expanded={nestExpanded}
              >
                <div class="workshop-rail-dest-row">
                  {#if nest.length > 0}
                    <button
                      type="button"
                      class="workshop-rail-dest-twist-btn"
                      title={nestExpanded ? "Collapse" : "Expand"}
                      aria-label={nestExpanded
                        ? `Collapse ${navLabel(surface)}`
                        : `Expand ${navLabel(surface)}`}
                      aria-expanded={nestExpanded}
                      onclick={(event) => toggleNest(surface.id, event)}
                    >
                      <ChevronRight
                        size={12}
                        strokeWidth={2}
                        class="workshop-rail-dest-chevron {nestExpanded
                          ? 'workshop-rail-dest-chevron-open'
                          : ''}"
                      />
                    </button>
                  {:else}
                    <span
                      class="workshop-rail-dest-twist workshop-rail-dest-twist-empty"
                      aria-hidden="true"
                    ></span>
                  {/if}
                  <button
                    type="button"
                    data-rail-surface={surface.id}
                    class="{railBtnClass(surface.id, 'life', {
                      quietActive: true,
                      active: doorActive,
                    })} workshop-rail-dest-btn"
                    class:workshop-rail-dest-btn-dimmed={leafActive && nestExpanded}
                    class:workshop-rail-library-btn={isLibrary || isAutomations}
                    title={navTitle(surface)}
                    aria-label={badge > 0 ? `${navTitle(surface)} (${badge} active)` : navTitle(surface)}
                    aria-current={doorActive && !leafActive ? "page" : undefined}
                    aria-expanded={isLibrary
                      ? surfacePopoverOpen("library") ||
                        (railPopover?.kind === "lme" && isLmeLibraryMode(railPopover.mode))
                      : isAutomations
                        ? surfacePopoverOpen("automations") ||
                          (railPopover?.kind === "lme" &&
                            isLmeAutomationsMode(railPopover.mode))
                        : surfacePopoverOpen(surface.id)}
                    aria-haspopup={surfaceHasShellSidebarView(surface.id) ||
                    isLibrary ||
                    isAutomations
                      ? "dialog"
                      : undefined}
                    onclick={(event) => selectDestination(surface.id, event)}
                  >
                    <span class="workshop-rail-btn-icon" aria-hidden="true">
                      <Icon {...(hero ? heroIconProps : treeIconProps)} />
                      {#if badge > 0 && showCountBadge(surface.id)}
                        <span class="workshop-rail-count-badge">{badge > 9 ? "9+" : badge}</span>
                      {:else if badge > 0}
                        <span class="workshop-rail-badge"></span>
                      {:else if feedBadge !== "none"}
                        <span
                          class="workshop-rail-feed-badge workshop-rail-feed-badge-{feedBadge}"
                          title={feedBadge === "live" ? "Live feed" : "Stale feed"}
                        ></span>
                      {/if}
                    </span>
                    <span class="workshop-rail-btn-label">{navLabel(surface)}</span>
                  </button>
                </div>
                {#if nestExpanded}
                  <ul class="workshop-rail-nest" aria-label="{navLabel(surface)} recent">
                    {#each nest as nestItem (nestItem.id)}
                      <li>
                        <button
                          type="button"
                          class="workshop-rail-nest-btn"
                          class:workshop-rail-nest-btn-active={nestItemIsActive(
                            surface.id,
                            nestItem.id,
                          )}
                          class:workshop-rail-nest-btn-accent={nestItem.accent}
                          title={nestItem.meta
                            ? `${nestItem.label} · ${nestItem.meta}`
                            : nestItem.label}
                          onclick={() => void openNestItem(surface.id, nestItem)}
                        >
                          {#if nestItem.accent}
                            <span class="workshop-rail-nest-dot" aria-hidden="true"></span>
                          {/if}
                          <span class="workshop-rail-nest-label">{nestItem.label}</span>
                          {#if nestItem.meta}
                            <span class="workshop-rail-nest-meta">{nestItem.meta}</span>
                          {/if}
                        </button>
                      </li>
                    {/each}
                    {#if nest.length >= NAV_RAIL_NEST_LIMIT}
                      <li>
                        <button
                          type="button"
                          class="workshop-rail-nest-more"
                          onclick={(event) => selectDestination(surface.id, event)}
                        >
                          More
                        </button>
                      </li>
                    {/if}
                  </ul>
                {/if}
              </div>
            {/if}
          {/snippet}

          <div class="workshop-rail-primary">
            {#each lifeRail.primary as item, index (item.id)}
              {#if lifeRail.focusStartIndex > 0 && index === lifeRail.focusStartIndex}
                <div class="workshop-rail-breath" aria-hidden="true"></div>
              {/if}
              {#if lifeRail.customStartIndex >= 0 && index === lifeRail.customStartIndex}
                {#if lifeRail.showLibrary}
                  {@render railDest(
                    { kind: "surface", id: "library", surface: libraryRailSurface() },
                    false,
                  )}
                {/if}
                {#if lifeRail.showAutomations}
                  {@render railDest(
                    {
                      kind: "surface",
                      id: "automations",
                      surface: automationsRailSurface(),
                    },
                    false,
                  )}
                {/if}
                <div class="workshop-rail-breath" aria-hidden="true"></div>
              {/if}
              {@render railDest(item, item.id === "chat")}
            {/each}
            {#if (lifeRail.showLibrary || lifeRail.showAutomations) && lifeRail.customStartIndex < 0}
              {#if lifeRail.showLibrary}
                {@render railDest(
                  { kind: "surface", id: "library", surface: libraryRailSurface() },
                  false,
                )}
              {/if}
              {#if lifeRail.showAutomations}
                {@render railDest(
                  {
                    kind: "surface",
                    id: "automations",
                    surface: automationsRailSurface(),
                  },
                  false,
                )}
              {/if}
            {/if}
          </div>
        </div>

        <div class="workshop-rail-dock">
          {#if lifeRail.you.kind === "surface"}
            {@const youSurface = lifeRail.you.surface}
            {@const YouIcon = environmentIcon(youSurface.icon)}
            {@const youNest = youNestItems()}
            {@const youNestExpanded = youNest.length > 0 && isNestExpanded(YOU_NEST_KEY)}
            {@const contextActive =
              active === "context" || surfacePopoverOpen("context")}
            <div
              class="workshop-rail-dest workshop-rail-dock-you"
              class:workshop-rail-dest-has-nest={youNest.length > 0}
              class:workshop-rail-dest-expanded={youNestExpanded}
            >
              <div class="workshop-rail-dest-row">
                {#if youNest.length > 0}
                  <button
                    type="button"
                    class="workshop-rail-dest-twist-btn"
                    title={youNestExpanded ? "Collapse" : "Expand"}
                    aria-label={youNestExpanded ? "Collapse You" : "Expand You"}
                    aria-expanded={youNestExpanded}
                    onclick={(event) => toggleNest(YOU_NEST_KEY, event)}
                  >
                    <ChevronRight
                      size={12}
                      strokeWidth={2}
                      class="workshop-rail-dest-chevron {youNestExpanded
                        ? 'workshop-rail-dest-chevron-open'
                        : ''}"
                    />
                  </button>
                {:else}
                  <span
                    class="workshop-rail-dest-twist workshop-rail-dest-twist-empty"
                    aria-hidden="true"
                  ></span>
                {/if}
                <button
                  type="button"
                  class="{railBtnClass('profiles', 'utility', {
                    quietActive: true,
                    active:
                      active === 'profiles' ||
                      contextActive ||
                      surfacePopoverOpen('profiles'),
                  })} workshop-rail-dock-btn workshop-rail-dest-btn"
                  class:workshop-rail-dest-btn-dimmed={contextActive && youNestExpanded}
                  title="You — {activeProfileLabel}"
                  aria-label="You ({activeProfileLabel})"
                  aria-current={active === "profiles" ? "page" : undefined}
                  onclick={(event) => selectDestination("profiles", event)}
                >
                  <span class="workshop-rail-btn-icon" aria-hidden="true">
                    <YouIcon {...utilityIconProps} />
                  </span>
                  <span class="workshop-rail-btn-label">You</span>
                </button>
              </div>
              {#if youNestExpanded}
                <ul class="workshop-rail-nest" aria-label="Memory">
                  {#each youNest as nestItem (nestItem.id)}
                    <li>
                      <button
                        type="button"
                        class="workshop-rail-nest-btn"
                        class:workshop-rail-nest-btn-active={contextActive}
                        title={nestItem.label}
                        onclick={(event) => selectDestination(nestItem.id, event)}
                      >
                        <span class="workshop-rail-nest-label">{nestItem.label}</span>
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}

          <button
            type="button"
            data-rail-surface={SAFETY_SURFACE_SETTINGS}
            class="{railBtnClass(SAFETY_SURFACE_SETTINGS, 'utility', {
              quietActive: true,
              active:
                active === SAFETY_SURFACE_SETTINGS ||
                surfacePopoverOpen(SAFETY_SURFACE_SETTINGS),
            })} workshop-rail-dock-btn"
            title="Settings"
            aria-label="Settings"
            aria-current={active === SAFETY_SURFACE_SETTINGS ? "page" : undefined}
            aria-expanded={surfacePopoverOpen(SAFETY_SURFACE_SETTINGS)}
            aria-haspopup="dialog"
            onclick={(event) => selectDestination(SAFETY_SURFACE_SETTINGS, event)}
          >
            <span class="workshop-rail-btn-icon" aria-hidden="true">
              <Settings {...utilityIconProps} />
            </span>
            <span class="workshop-rail-btn-label">Settings</span>
          </button>
        </div>
      {/if}
    </div>
  {/key}
</nav>

{#if railPopover}
  <NavRailViewPopover
    open={railPopoverOpen}
    title={railPopoverTitle}
    targetKey={railPopoverTargetKey}
    triggerEl={railPopoverTriggerEl}
    cursorAnchor={railPopoverCursor}
    preferPhase={railPopoverPreferPhase}
    onClose={closeRailPopover}
    onDockToRail={dockPopoverToRail}
    dockHost={railPopoverUsesLmeDock}
  >
    {#snippet toolbar()}
      {#if railPopover.kind === "lme"}
        <!-- LME dock icons portal into the popover dock slot. -->
      {:else if railPopover.surfaceId === "library" || railPopover.surfaceId === "automations"}
        <!-- LME dock icons portal into the popover dock slot. -->
      {:else if railPopover.surfaceId === "chat"}
        <SessionRailToolbar onCreated={closeRailPopover} />
      {:else if railPopover.surfaceId === "peers"}
        <PeersRailToolbar />
      {:else if railPopover.surfaceId === "messaging"}
        <MessagingRailToolbar />
      {:else if railPopover.surfaceId === "context"}
        <div class="nav-rail-context-toolbar">
          <ContextModeBar />
        </div>
      {:else if railPopover.surfaceId === SAFETY_SURFACE_SETTINGS}
        <span class="nav-rail-popover-toolbar-label">Settings</span>
      {/if}
    {/snippet}

    {#if railPopover.kind === "lme"}
      <LmeSidePanel
        {onOpenChat}
        family={isLmeAutomationsMode(railPopover.mode) ? "automations" : "library"}
      />
    {:else if railPopover.surfaceId === "library" || railPopover.surfaceId === "automations"}
      <LmeSidePanel {onOpenChat} family={lmeFamilyForSurface(railPopover.surfaceId)} />
    {:else if railPopover.surfaceId === SAFETY_SURFACE_SETTINGS}
      <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1">
        <SettingsNav
          active={settingsNav.activeSection}
          onSelect={(section) => {
            settingsNav.setActiveSection(section);
            commitPopoverSurface(SAFETY_SURFACE_SETTINGS);
          }}
        />
      </div>
    {:else if railPopover.surfaceId === "chat"}
      <SessionSidebar
        open={true}
        variant="inline"
        chrome="rail-list"
        onPick={closeRailPopover}
      />
    {:else if railPopover.surfaceId === "messaging"}
      <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-2">
        <MessagingChannelList
          search={messagingShell.search}
          selected={messagingShell.selectedChannel}
          summary={messaging.summary}
          {daemonOk}
          loading={messaging.loading}
          error={messaging.error}
          onSelect={(id) => {
            messagingShell.selectChannel(id);
            commitPopoverSurface("messaging");
          }}
        />
      </div>
    {:else if railPopover.surfaceId === "peers"}
      <PeersShellList
        chrome="rail-list"
        onPickPeer={() => commitPopoverSurface("peers")}
      />
    {:else if railPopover.surfaceId === "context"}
      <ContextSidePanel
        chrome="rail-list"
        onPick={() => commitPopoverSurface("context")}
      />
    {/if}
  </NavRailViewPopover>
{/if}

<style>
  :global(.nav-rail-context-toolbar) {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    justify-content: flex-start;
  }

  :global(.nav-rail-context-toolbar .lme-side-mode-bar) {
    width: auto;
    border-bottom: 0;
    padding: 0;
  }

  :global(.nav-rail-context-toolbar .lme-side-mode-bar > .flex-1) {
    flex: 0 1 auto;
  }

  :global(.nav-rail-popover-toolbar-label) {
    padding: 0 0.35rem;
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-200));
  }

  .master-rail-root {
    position: relative;
  }

  .master-rail-mode {
    position: absolute;
    inset: 0;
    display: flex;
    min-height: 0;
    flex-direction: column;
  }

  .master-rail-header {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 2.35rem;
    margin-bottom: 0.25rem;
    padding: 0.25rem 0.55rem 0.35rem 0.55rem;
    border-bottom: 1px solid rgb(var(--shell-border) / 0.28);
  }

  .master-rail-header-nav {
    justify-content: flex-end;
  }

  .master-rail-nav-spacer {
    flex: 1;
  }

  .master-rail-back {
    display: inline-flex;
    align-items: center;
    gap: 0.05rem;
    padding: 0.2rem 0.35rem 0.2rem 0.1rem;
    border-radius: 0.375rem;
    color: rgb(var(--color-surface-300));
    font-size: 0.75rem;
    font-weight: 600;
  }

  .master-rail-back:hover {
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    color: rgb(var(--color-surface-100));
  }

  .master-rail-view-title {
    margin: 0;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .master-rail-collapse {
    display: inline-flex;
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.375rem;
    color: rgb(var(--color-surface-500));
  }

  .master-rail-collapse:hover {
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    color: rgb(var(--color-surface-200));
  }

  .master-rail-view-body {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
  }
</style>
