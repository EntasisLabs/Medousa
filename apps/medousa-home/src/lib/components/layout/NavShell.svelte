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
  import WebRailToolbar from "$lib/components/browser/WebRailToolbar.svelte";
  import WebRailList from "$lib/components/browser/WebRailList.svelte";
  import CalendarRailToolbar from "$lib/components/calendar/CalendarRailToolbar.svelte";
  import CalendarRailList from "$lib/components/calendar/CalendarRailList.svelte";
  import YouRailToolbar from "$lib/components/profiles/YouRailToolbar.svelte";
  import YouRailList from "$lib/components/profiles/YouRailList.svelte";
  import WorkRailToolbar from "$lib/components/work/WorkRailToolbar.svelte";
  import WorkRailList from "$lib/components/work/WorkRailList.svelte";
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
    registerRailPopoverSummon,
    setLastPointer,
    type RailPopoverCursor,
  } from "$lib/utils/railPopoverSummon";
  import { resolveSummonToolbarSurface } from "$lib/utils/resolveSummonToolbarSurface";
  import { toast } from "$lib/stores/toast.svelte";
  import { Settings } from "@lucide/svelte";
  import { SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";
  import type { DaemonHealth } from "$lib/daemon";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { onMount } from "svelte";

  type RailPopoverTarget =
    | { kind: "lme"; mode: LmeExplorerMode }
    | { kind: "surface"; surfaceId: string };

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
      lmeWorkspace.activeTab?.kind ?? null,
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

  onMount(() => {
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
          {:else if viewSurface === "web"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="min-h-0 flex-1 overflow-hidden">
                <WebRailList onPickTab={() => onSelect("web")} />
              </div>
              <div class="lme-side-rail-dock">
                <WebRailToolbar onNavigated={() => onSelect("web")} />
              </div>
            </div>
          {:else if viewSurface === "calendar"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="min-h-0 flex-1 overflow-hidden">
                <CalendarRailList onPickEvent={() => onSelect("calendar")} />
              </div>
              <div class="lme-side-rail-dock">
                <CalendarRailToolbar onAction={() => onSelect("calendar")} />
              </div>
            </div>
          {:else if viewSurface === "work"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="min-h-0 flex-1 overflow-hidden">
                <WorkRailList onPickCard={() => onSelect("work")} />
              </div>
              <div class="lme-side-rail-dock">
                <WorkRailToolbar onAction={() => onSelect("work")} />
              </div>
            </div>
          {:else if viewSurface === "profiles"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="min-h-0 flex-1 overflow-hidden">
                <YouRailList
                  onPickProfile={() => onSelect("profiles")}
                  onOpenContext={() => selectDestination("context")}
                />
              </div>
              <div class="lme-side-rail-dock">
                <YouRailToolbar
                  onAction={() => onSelect("profiles")}
                  onOpenContext={() => selectDestination("context")}
                />
              </div>
            </div>
          {/if}
        </div>
      {:else}
        <div
          class="workshop-icon-rail-items workshop-rail-tree workshop-rail-tree-jobs flex min-h-0 flex-1 flex-col overflow-y-auto"
        >
          {#snippet railDest(item: LifeRailItem, hero = false)}
            {#if item.kind === "surface"}
              {@const surface = item.surface}
              {@const Icon = environmentIcon(surface.icon)}
              {@const badge = activityFor(surface.id)}
              {@const feedBadge = feedBadgeForSurface(surface)}
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
              >
                <div class="workshop-rail-dest-row">
                  <button
                    type="button"
                    data-rail-surface={surface.id}
                    class="{railBtnClass(surface.id, 'life', {
                      quietActive: true,
                      active: doorActive,
                    })} workshop-rail-dest-btn"
                    class:workshop-rail-library-btn={isLibrary || isAutomations}
                    title={navTitle(surface)}
                    aria-label={badge > 0 ? `${navTitle(surface)} (${badge} active)` : navTitle(surface)}
                    aria-current={doorActive ? "page" : undefined}
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
          {#if lifeRail.context?.kind === "surface"}
            {@const contextSurface = lifeRail.context.surface}
            {@const ContextIcon = environmentIcon(contextSurface.icon)}
            {@const contextDoorActive =
              active === "context" ||
              surfacePopoverOpen("context") ||
              (showView && viewSurface === "context")}
            <button
              type="button"
              data-rail-surface="context"
              class="{railBtnClass('context', 'utility', {
                quietActive: true,
                active: contextDoorActive,
              })} workshop-rail-dock-btn"
              title={navTitle(contextSurface)}
              aria-label={navLabel(contextSurface)}
              aria-current={contextDoorActive ? "page" : undefined}
              aria-expanded={surfacePopoverOpen("context")}
              aria-haspopup="dialog"
              onclick={(event) => selectDestination("context", event)}
            >
              <span class="workshop-rail-btn-icon" aria-hidden="true">
                <ContextIcon {...utilityIconProps} />
              </span>
              <span class="workshop-rail-btn-label">{navLabel(contextSurface)}</span>
            </button>
          {/if}

          {#if lifeRail.you.kind === "surface"}
            {@const youSurface = lifeRail.you.surface}
            {@const YouIcon = environmentIcon(youSurface.icon)}
            <button
              type="button"
              data-rail-surface="profiles"
              class="{railBtnClass('profiles', 'utility', {
                quietActive: true,
                active: active === 'profiles' || surfacePopoverOpen('profiles'),
              })} workshop-rail-dock-btn"
              title="You — {activeProfileLabel}"
              aria-label="You ({activeProfileLabel})"
              aria-current={active === "profiles" ? "page" : undefined}
              aria-expanded={surfacePopoverOpen("profiles")}
              aria-haspopup="dialog"
              onclick={(event) => selectDestination("profiles", event)}
            >
              <span class="workshop-rail-btn-icon" aria-hidden="true">
                <YouIcon {...utilityIconProps} />
              </span>
              <span class="workshop-rail-btn-label">You</span>
            </button>
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
      {:else if railPopover.surfaceId === "web"}
        <WebRailToolbar onNavigated={() => commitPopoverSurface("web")} />
      {:else if railPopover.surfaceId === "calendar"}
        <CalendarRailToolbar onAction={() => commitPopoverSurface("calendar")} />
      {:else if railPopover.surfaceId === "work"}
        <WorkRailToolbar onAction={() => commitPopoverSurface("work")} />
      {:else if railPopover.surfaceId === "profiles"}
        <YouRailToolbar
          onAction={() => commitPopoverSurface("profiles")}
          onOpenContext={() => {
            closeRailPopover();
            selectDestination("context");
          }}
        />
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
    {:else if railPopover.surfaceId === "web"}
      <WebRailList onPickTab={() => commitPopoverSurface("web")} />
    {:else if railPopover.surfaceId === "calendar"}
      <CalendarRailList onPickEvent={() => commitPopoverSurface("calendar")} />
    {:else if railPopover.surfaceId === "work"}
      <WorkRailList onPickCard={() => commitPopoverSurface("work")} />
    {:else if railPopover.surfaceId === "profiles"}
      <YouRailList
        onPickProfile={() => commitPopoverSurface("profiles")}
        onOpenContext={() => {
          closeRailPopover();
          selectDestination("context");
        }}
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

  .master-rail-view-body {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
  }
</style>
