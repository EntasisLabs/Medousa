<script lang="ts">
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import MapSidePanel from "$lib/components/context/MapSidePanel.svelte";
  import MapRailToolbar from "$lib/components/context/MapRailToolbar.svelte";
  import SessionRailToolbar from "$lib/components/chat/SessionRailToolbar.svelte";
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
  import CanvasAddViewForm from "$lib/components/settings/CanvasAddViewForm.svelte";
  import CanvasEditViewPopover from "$lib/components/settings/CanvasEditViewPopover.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace, type LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";
  import { messaging } from "$lib/stores/messaging.svelte";
  import { messagingShell } from "$lib/stores/messagingShell.svelte";
  import { appUpdate } from "$lib/stores/appUpdate.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import type { SettingsSectionId } from "$lib/types/settings";
  import { feedBadgeForComponents } from "$lib/utils/customViewStatus";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    isNavDestinationToggleable,
    NAV_DESTINATION_GROUPS,
  } from "$lib/utils/environmentLayout";
  import type { SurfaceDef } from "$lib/types/environment";
  import {
    defaultModeForLmeFamily,
    isLmeAutomationsMode,
    isLmeLibraryMode,
    labelForLmeExplorerMode,
    type LmeExplorerFamily,
  } from "$lib/utils/lmeExplorerModes";
  import { buildLifeRailLayout } from "$lib/utils/lifeRailSections";
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
  import { Check, GripVertical, Minus, Pencil, Plus, Search, Settings } from "@lucide/svelte";
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
    viewSurface === "library" ||
      viewSurface === "notes" ||
      viewSurface === "files" ||
      viewSurface === "artifacts" ||
      viewSurface === "automations" ||
      viewSurface === "code"
      ? labelForLmeExplorerMode(lmeWorkspace.explorerMode)
      : shellSidebarViewTitle(viewSurface),
  );
  const daemonOk = $derived(health?.ok ?? false);
  const settingsNavBadges = $derived.by(() => {
    const badges: Partial<Record<SettingsSectionId, number>> = {};
    if (appUpdate.updateAvailable) badges.basement = 1;
    return badges;
  });

  const surfaces = $derived(environment.navSurfaces());
  const lifeRail = $derived(buildLifeRailLayout(surfaces));
  const railLayoutEditing = $derived(layout.railLayoutEditing);
  const navIdSet = $derived(new Set(surfaces.map((surface) => surface.id)));
  const librarySurfaces = $derived.by(() => {
    const spec = environment.spec;
    if (!spec) return [] as SurfaceDef[];
    const byId = new Map(spec.surfaces.map((surface) => [surface.id, surface]));
    const ordered: string[] = [];
    const seen = new Set<string>();
    for (const group of NAV_DESTINATION_GROUPS) {
      for (const id of group.surfaceIds) {
        if (seen.has(id)) continue;
        seen.add(id);
        ordered.push(id);
      }
    }
    for (const surface of spec.surfaces) {
      if (!isNavDestinationToggleable(surface.id) || seen.has(surface.id)) continue;
      seen.add(surface.id);
      ordered.push(surface.id);
    }
    return ordered
      .filter((id) => !navIdSet.has(id) && byId.has(id))
      .map((id) => byId.get(id)!)
      .filter((surface) => isNavDestinationToggleable(surface.id));
  });

  let railLayoutBusy = $state(false);
  let editingCustomSurfaceId = $state<string | null>(null);
  let editingCustomAnchorEl = $state<HTMLElement | null>(null);
  let railDragId = $state<string | null>(null);
  /** Insertion index in the primary strip while dragging (0…length). */
  let railInsertIndex = $state<number | null>(null);
  let railPrimaryEl = $state<HTMLElement | null>(null);

  /** Non-reactive drag session — pointer handlers must not depend on Svelte batching. */
  let railDragSession: {
    surfaceId: string;
    fromIndex: number;
    pointerId: number;
  } | null = null;

  const editingCustomSurface = $derived(
    editingCustomSurfaceId
      ? (environment.spec?.surfaces.find((surface) => surface.id === editingCustomSurfaceId) ??
        null)
      : null,
  );

  $effect(() => {
    if (!railLayoutEditing) {
      editingCustomSurfaceId = null;
      editingCustomAnchorEl = null;
      clearRailDrag();
    }
  });

  function closeCustomViewEdit() {
    editingCustomSurfaceId = null;
    editingCustomAnchorEl = null;
  }

  async function setRailNavVisible(surfaceId: string, visible: boolean) {
    if (railLayoutBusy) return;
    railLayoutBusy = true;
    try {
      await environment.setSurfaceNavVisible(surfaceId, visible);
      if (!visible && layout.desktopSurface === surfaceId) {
        const fallback =
          environment.navSurfaces().find((surface) => surface.id !== surfaceId)?.id ?? "chat";
        onSelect(fallback);
      }
    } catch (err) {
      toast.show(err instanceof Error ? err.message : String(err), { durationMs: 2400 });
    } finally {
      railLayoutBusy = false;
    }
  }

  function clearRailDrag() {
    railDragSession = null;
    railDragId = null;
    railInsertIndex = null;
    document.body.classList.remove("workshop-rail-dragging");
  }

  function primaryRowEls(): HTMLElement[] {
    if (!railPrimaryEl) return [];
    return Array.from(
      railPrimaryEl.querySelectorAll<HTMLElement>("[data-rail-reorder-id]"),
    );
  }

  function insertIndexAtY(clientY: number): number {
    const rows = primaryRowEls();
    if (rows.length === 0) return 0;
    for (let i = 0; i < rows.length; i++) {
      const rect = rows[i]!.getBoundingClientRect();
      const mid = rect.top + rect.height / 2;
      if (clientY < mid) return i;
    }
    return rows.length;
  }

  function onRailReorderPointerMove(event: PointerEvent) {
    const session = railDragSession;
    if (!session || event.pointerId !== session.pointerId) return;
    railInsertIndex = insertIndexAtY(event.clientY);
  }

  function onRailReorderPointerUp(event: PointerEvent) {
    const session = railDragSession;
    if (!session || event.pointerId !== session.pointerId) return;
    const from = session.fromIndex;
    const surfaceId = session.surfaceId;
    // Insert index is among current rows; convert to final primary index.
    const insertAt = railInsertIndex ?? insertIndexAtY(event.clientY);
    document.removeEventListener("pointermove", onRailReorderPointerMove);
    document.removeEventListener("pointerup", onRailReorderPointerUp);
    document.removeEventListener("pointercancel", onRailReorderPointerUp);
    clearRailDrag();

    // When dragging downward, the slot after removal shifts left by one.
    let to = insertAt;
    if (to > from) to -= 1;
    if (to === from || railLayoutBusy) return;

    railLayoutBusy = true;
    void environment
      .reorderPrimarySurfaceInNav(surfaceId, to)
      .catch((err) => {
        toast.show(err instanceof Error ? err.message : String(err), { durationMs: 2400 });
      })
      .finally(() => {
        railLayoutBusy = false;
      });
  }

  function onRailReorderPointerDown(
    surfaceId: string,
    fromIndex: number,
    event: PointerEvent,
  ) {
    if (!railLayoutEditing || railLayoutBusy || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    railDragSession = {
      surfaceId,
      fromIndex,
      pointerId: event.pointerId,
    };
    railDragId = surfaceId;
    railInsertIndex = fromIndex;
    document.body.classList.add("workshop-rail-dragging");
    document.addEventListener("pointermove", onRailReorderPointerMove);
    document.addEventListener("pointerup", onRailReorderPointerUp);
    document.addEventListener("pointercancel", onRailReorderPointerUp);
    try {
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    } catch {
      /* capture is best-effort */
    }
  }

  /** Quieter tree parents — closer to Cursor folder icons. */
  const treeIconProps = { size: 14, strokeWidth: 2 };
  const heroIconProps = { size: 17, strokeWidth: 2.15 };
  const utilityIconProps = { size: 14, strokeWidth: 2 };
  /** In-place flyout — replaces rail view-mode swaps for list surfaces. */
  let railPopover = $state<RailPopoverTarget | null>(null);
  let railPopoverTriggerEl = $state<HTMLElement | null>(null);
  /** Click point so the toolbar floats next to the mouse (not rail-docked). */
  let railPopoverCursor = $state<{ x: number; y: number } | null>(null);
  /** Landing phase — shake / keybind summon only (no rail hover). */
  let railPopoverPreferPhase = $state<"seed" | "toolbar">("toolbar");
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
      railPopover?.surfaceId === "notes" ||
      railPopover?.surfaceId === "files" ||
      railPopover?.surfaceId === "artifacts" ||
      railPopover?.surfaceId === "automations" ||
      railPopover?.surfaceId === "code",
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

  const LIBRARY_DOOR_MODES: Record<string, LmeExplorerMode> = {
    notes: "notes",
    files: "files",
    artifacts: "artifacts",
  };

  function libraryDoorIsActive(surfaceId: string): boolean {
    const mode = LIBRARY_DOOR_MODES[surfaceId];
    if (!mode) return false;
    if (surfacePopoverOpen(surfaceId)) return true;
    if (railPopover?.kind === "lme" && railPopover.mode === mode) return true;
    if (showView && viewSurface === surfaceId) return true;
    if (showView && viewSurface === "library" && lmeWorkspace.explorerMode === mode) {
      return true;
    }
    // Main content is the shared library host — highlight only the matching mode door.
    if (active === "library" || active === surfaceId) {
      return lmeWorkspace.explorerMode === mode;
    }
    return false;
  }

  function automationsIsActive(): boolean {
    if (surfacePopoverOpen("automations")) return true;
    if (railPopover?.kind === "lme" && isLmeAutomationsMode(railPopover.mode)) return true;
    if (showView && viewSurface === "automations") return true;
    if (active !== "library" && active !== "automations") return false;
    return isLmeAutomationsMode(lmeWorkspace.explorerMode);
  }

  function ensureFamilyForSurface(surfaceId: string) {
    const doorMode = LIBRARY_DOOR_MODES[surfaceId];
    if (doorMode) {
      lmeWorkspace.setExplorerMode(doorMode);
      return;
    }
    if (surfaceId === "code") {
      lmeWorkspace.setExplorerMode(defaultModeForLmeFamily("code"));
    } else if (surfaceId === "library" && !isLmeLibraryMode(lmeWorkspace.explorerMode)) {
      lmeWorkspace.setExplorerMode(defaultModeForLmeFamily("library"));
    } else if (
      surfaceId === "automations" &&
      !isLmeAutomationsMode(lmeWorkspace.explorerMode)
    ) {
      lmeWorkspace.setExplorerMode(defaultModeForLmeFamily("automations"));
    }
  }

  function lmeFamilyForSurface(surfaceId: string): LmeExplorerFamily {
    if (surfaceId === "code") return "code";
    if (surfaceId === "automations") return "automations";
    if (
      surfaceId === "library" ||
      surfaceId === "notes" ||
      surfaceId === "files" ||
      surfaceId === "artifacts"
    ) {
      return "library";
    }
    return "library";
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
    railPopoverPreferPhase = "toolbar";
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
    options?: {
      cursor?: RailPopoverCursor;
      preferPhase?: "seed" | "toolbar";
    },
  ) {
    if (sameRailPopover(target)) {
      closeRailPopover();
      return;
    }
    railPopoverPreferPhase = options?.preferPhase ?? "toolbar";
    railPopoverTriggerEl = trigger;
    if (options?.cursor) {
      railPopoverCursor = options.cursor;
    } else if (event) {
      railPopoverCursor = { x: event.clientX, y: event.clientY };
    } else {
      const rect = trigger.getBoundingClientRect();
      railPopoverCursor = {
        x: rect.right + 8,
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

  /**
   * Row click — host the list in the master rail when available, and open the
   * matching center tab/surface so switching doors actually changes context.
   * Custom destinations without a rail view just navigate.
   */
  function selectDestination(surfaceId: string, event?: MouseEvent) {
    event?.preventDefault();
    event?.stopPropagation();
    closeRailPopover();
    ensureFamilyForSurface(surfaceId);
    if (surfaceHasShellSidebarView(surfaceId)) {
      layout.openShellSidebarView(surfaceId);
      onSelect(surfaceId);
      return;
    }
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
      layout.openShellSidebarView(
        mode === "code"
          ? "code"
          : isLmeAutomationsMode(mode)
            ? "automations"
            : mode === "files"
              ? "files"
              : mode === "artifacts"
                ? "artifacts"
                : "notes",
      );
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
                badges={settingsNavBadges}
                onSelect={(section) => {
                  settingsNav.setActiveSection(section);
                  onSelect(SAFETY_SURFACE_SETTINGS);
                }}
              />
            </div>
          {:else if viewSurface === "chat"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="lme-side-rail-dock">
                <SessionRailToolbar variant="rail-row" onCreated={() => onSelect("chat")} />
              </div>
              <div class="min-h-0 flex-1 overflow-hidden">
                <SessionSidebar open={true} variant="inline" chrome="rail-list" />
              </div>
            </div>
          {:else if viewSurface === "library" ||
            viewSurface === "notes" ||
            viewSurface === "files" ||
            viewSurface === "artifacts" ||
            viewSurface === "automations" ||
            viewSurface === "code"}
            <LmeSidePanel {onOpenChat} family={lmeFamilyForSurface(viewSurface)} />
          {:else if viewSurface === "messaging"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <header class="lme-side-rail-dock">
                <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
                  <Search
                    size={14}
                    strokeWidth={1.75}
                    class="shrink-0 text-content-quiet"
                    aria-hidden="true"
                  />
                  <input
                    class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
                    type="search"
                    placeholder="Search channels…"
                    value={messagingShell.search}
                    oninput={(event) => {
                      messagingShell.search = (event.currentTarget as HTMLInputElement).value;
                    }}
                  />
                </div>
              </header>
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
          {:else if viewSurface === "map"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="lme-side-rail-dock">
                <MapRailToolbar onPick={() => onSelect("map")} />
              </div>
              <div class="min-h-0 flex-1 overflow-hidden">
                <MapSidePanel onPick={() => onSelect("map")} />
              </div>
            </div>
          {:else if viewSurface === "web"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="lme-side-rail-dock">
                <WebRailToolbar onNavigated={() => onSelect("web")} />
              </div>
              <div class="min-h-0 flex-1 overflow-hidden">
                <WebRailList onPickTab={() => onSelect("web")} />
              </div>
            </div>
          {:else if viewSurface === "calendar"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="lme-side-rail-dock">
                <CalendarRailToolbar onAction={() => onSelect("calendar")} />
              </div>
              <div class="min-h-0 flex-1 overflow-hidden">
                <CalendarRailList onPickEvent={() => onSelect("calendar")} />
              </div>
            </div>
          {:else if viewSurface === "work"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="lme-side-rail-dock">
                <WorkRailToolbar onAction={() => onSelect("work")} />
              </div>
              <div class="min-h-0 flex-1 overflow-hidden">
                <WorkRailList onPickCard={() => onSelect("work")} />
              </div>
            </div>
          {:else if viewSurface === "profiles"}
            <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="lme-side-rail-dock">
                <YouRailToolbar onAction={() => onSelect("profiles")} />
              </div>
              <div class="min-h-0 flex-1 overflow-hidden">
                <YouRailList onPickProfile={() => onSelect("profiles")} />
              </div>
            </div>
          {/if}
        </div>
      {:else}
        <div
          class="workshop-icon-rail-items workshop-rail-tree workshop-rail-tree-jobs flex min-h-0 flex-1 flex-col overflow-y-auto"
          class:workshop-rail-layout-editing={railLayoutEditing}
        >
          {#if railLayoutEditing}
            <div class="workshop-rail-layout-edit-bar">
              <span class="workshop-rail-layout-edit-label">Adjust rail</span>
              <button
                type="button"
                class="workshop-rail-layout-edit-done"
                onclick={() => layout.stopRailLayoutEditing()}
              >
                <Check size={14} strokeWidth={2.25} aria-hidden="true" />
                Done
              </button>
            </div>
          {/if}

          {#snippet railDest(item: LifeRailItem, hero = false, orderIndex = -1)}
            {#if item.kind === "surface"}
              {@const surface = item.surface}
              {@const Icon = environmentIcon(surface.icon)}
              {@const badge = activityFor(surface.id)}
              {@const feedBadge = feedBadgeForSurface(surface)}
              {@const isLibraryDoor =
                surface.id === "notes" ||
                surface.id === "files" ||
                surface.id === "artifacts"}
              {@const isAutomations = surface.id === "automations"}
              {@const doorActive = isLibraryDoor
                ? libraryDoorIsActive(surface.id)
                : isAutomations
                  ? automationsIsActive()
                  : active === surface.id || surfacePopoverOpen(surface.id)}
              {@const canHide =
                railLayoutEditing && isNavDestinationToggleable(surface.id)}
              {@const canEditCustom = canHide && surface.kind === "custom"}
              {@const canReorder = canHide && orderIndex >= 0}
              {@const isDragging = railDragId === surface.id}
              {@const showDropBefore =
                canReorder &&
                railDragId !== null &&
                railDragId !== surface.id &&
                railInsertIndex === orderIndex}
              <div
                class="workshop-rail-dest"
                class:workshop-rail-dest-hero={hero}
                class:workshop-rail-dest-dragging={isDragging}
                class:workshop-rail-dest-drop-before={showDropBefore}
                data-rail-reorder-id={canReorder ? surface.id : undefined}
              >
                <div class="workshop-rail-dest-row">
                  {#if canReorder}
                    <button
                      type="button"
                      class="workshop-rail-drag-handle"
                      title="Drag to reorder"
                      aria-label="Drag to reorder {navLabel(surface)}"
                      disabled={railLayoutBusy}
                      onpointerdown={(event) =>
                        onRailReorderPointerDown(surface.id, orderIndex, event)}
                    >
                      <GripVertical size={14} strokeWidth={2} />
                    </button>
                  {/if}
                  <button
                    type="button"
                    data-rail-surface={surface.id}
                    class="{railBtnClass(surface.id, 'life', {
                      quietActive: true,
                      active: doorActive,
                    })} workshop-rail-dest-btn"
                    class:workshop-rail-library-btn={isLibraryDoor || isAutomations}
                    title={navTitle(surface)}
                    aria-label={badge > 0 ? `${navTitle(surface)} (${badge} active)` : navTitle(surface)}
                    aria-current={doorActive ? "page" : undefined}
                    aria-expanded={showView && viewSurface === surface.id}
                    onclick={(event) => {
                      if (railLayoutEditing) {
                        event.preventDefault();
                        return;
                      }
                      selectDestination(surface.id, event);
                    }}
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
                  {#if canHide}
                    <div class="workshop-rail-dest-actions">
                      {#if canEditCustom}
                        <button
                          type="button"
                          class="vault-dock-icon-btn workshop-rail-row-action"
                          class:workshop-rail-row-action-open={editingCustomSurfaceId === surface.id}
                          title="Edit view"
                          aria-label="Edit {navLabel(surface)}"
                          aria-expanded={editingCustomSurfaceId === surface.id}
                          disabled={railLayoutBusy}
                          onclick={(event) => {
                            event.stopPropagation();
                            const button = event.currentTarget as HTMLElement;
                            if (editingCustomSurfaceId === surface.id) {
                              closeCustomViewEdit();
                              return;
                            }
                            editingCustomSurfaceId = surface.id;
                            editingCustomAnchorEl = button;
                          }}
                        >
                          <Pencil size={13} strokeWidth={2} />
                        </button>
                      {/if}
                      <button
                        type="button"
                        class="vault-dock-icon-btn workshop-rail-row-action"
                        title="Remove from layout"
                        aria-label="Remove {navLabel(surface)} from layout"
                        disabled={railLayoutBusy}
                        onclick={(event) => {
                          event.stopPropagation();
                          void setRailNavVisible(surface.id, false);
                        }}
                      >
                        <Minus size={14} strokeWidth={2} />
                      </button>
                    </div>
                  {/if}
                </div>
              </div>
            {/if}
          {/snippet}

          <div
            class="workshop-rail-primary"
            class:workshop-rail-primary-dragging={railDragId !== null}
            bind:this={railPrimaryEl}
          >
            {#each lifeRail.primary as item, index (item.id)}
              {#if lifeRail.focusStartIndex > 0 && index === lifeRail.focusStartIndex}
                <div class="workshop-rail-breath" aria-hidden="true"></div>
              {:else if lifeRail.customStartIndex >= 0 && index === lifeRail.customStartIndex}
                <div class="workshop-rail-breath" aria-hidden="true"></div>
              {/if}
              {@render railDest(item, item.id === "chat", index)}
            {/each}
            {#if railLayoutEditing && railDragId !== null}
              <div
                class="workshop-rail-drop-end"
                class:workshop-rail-dest-drop-before={railInsertIndex === lifeRail.primary.length}
                aria-hidden="true"
              ></div>
            {/if}
          </div>

          {#if railLayoutEditing}
            <div class="workshop-rail-layout-library">
              <p class="workshop-rail-layout-library-label">Available</p>
              {#if librarySurfaces.length === 0}
                <p class="workshop-rail-layout-library-empty">Nothing left to add</p>
              {:else}
                {#each librarySurfaces as surface (surface.id)}
                  {@const Icon = environmentIcon(surface.icon)}
                  <button
                    type="button"
                    class="workshop-rail-btn workshop-rail-btn-tier-life workshop-rail-layout-library-row"
                    disabled={railLayoutBusy}
                    title="Add {surface.label} to layout"
                    onclick={() => void setRailNavVisible(surface.id, true)}
                  >
                    <span class="workshop-rail-btn-icon" aria-hidden="true">
                      <Icon {...treeIconProps} />
                    </span>
                    <span class="workshop-rail-btn-label">{surface.label}</span>
                    <Plus size={14} strokeWidth={2} class="workshop-rail-layout-library-plus" aria-hidden="true" />
                  </button>
                {/each}
              {/if}
              <div class="workshop-rail-layout-add-view">
                <CanvasAddViewForm />
              </div>
            </div>
          {/if}

          {#if railLayoutEditing && editingCustomSurface}
            <CanvasEditViewPopover
              surface={editingCustomSurface}
              anchorEl={editingCustomAnchorEl}
              onClose={closeCustomViewEdit}
              onSaved={closeCustomViewEdit}
              onDeleted={closeCustomViewEdit}
            />
          {/if}
        </div>

        <div class="workshop-rail-dock">
          {#if lifeRail.you.kind === "surface"}
            {@const YouIcon = environmentIcon(lifeRail.you.surface.icon)}
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
              aria-expanded={showView && viewSurface === "profiles"}
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
            aria-expanded={showView && viewSurface === SAFETY_SURFACE_SETTINGS}
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
  {@const popover = railPopover}
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
      {#if popover.kind === "lme"}
        <!-- LME dock icons portal into the popover dock slot. -->
      {:else if popover.surfaceId === "library" ||
        popover.surfaceId === "notes" ||
        popover.surfaceId === "files" ||
        popover.surfaceId === "artifacts" ||
        popover.surfaceId === "automations" ||
        popover.surfaceId === "code"}
        <!-- LME dock icons portal into the popover dock slot. -->
      {:else if popover.surfaceId === "chat"}
        <SessionRailToolbar onCreated={closeRailPopover} />
      {:else if popover.surfaceId === "peers"}
        <PeersRailToolbar />
      {:else if popover.surfaceId === "messaging"}
        <MessagingRailToolbar />
      {:else if popover.surfaceId === "map"}
        <MapRailToolbar onPick={() => commitPopoverSurface("map")} />
      {:else if popover.surfaceId === "web"}
        <WebRailToolbar onNavigated={() => commitPopoverSurface("web")} />
      {:else if popover.surfaceId === "calendar"}
        <CalendarRailToolbar onAction={() => commitPopoverSurface("calendar")} />
      {:else if popover.surfaceId === "work"}
        <WorkRailToolbar onAction={() => commitPopoverSurface("work")} />
      {:else if popover.surfaceId === "profiles"}
        <YouRailToolbar onAction={() => commitPopoverSurface("profiles")} />
      {:else if popover.surfaceId === SAFETY_SURFACE_SETTINGS}
        <span class="nav-rail-popover-toolbar-label">Settings</span>
      {/if}
    {/snippet}

    {#if popover.kind === "lme"}
      <LmeSidePanel
        {onOpenChat}
        family={popover.mode === "code"
          ? "code"
          : isLmeAutomationsMode(popover.mode)
            ? "automations"
            : "library"}
      />
    {:else if popover.surfaceId === "library" ||
      popover.surfaceId === "notes" ||
      popover.surfaceId === "files" ||
      popover.surfaceId === "artifacts" ||
      popover.surfaceId === "automations" ||
      popover.surfaceId === "code"}
      <LmeSidePanel {onOpenChat} family={lmeFamilyForSurface(popover.surfaceId)} />
    {:else if popover.surfaceId === SAFETY_SURFACE_SETTINGS}
      <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1">
        <SettingsNav
          active={settingsNav.activeSection}
          badges={settingsNavBadges}
          onSelect={(section) => {
            settingsNav.setActiveSection(section);
            commitPopoverSurface(SAFETY_SURFACE_SETTINGS);
          }}
        />
      </div>
    {:else if popover.surfaceId === "chat"}
      <SessionSidebar
        open={true}
        variant="inline"
        chrome="rail-list"
        onPick={closeRailPopover}
      />
    {:else if popover.surfaceId === "messaging"}
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
    {:else if popover.surfaceId === "peers"}
      <PeersShellList
        chrome="rail-list"
        onPickPeer={() => commitPopoverSurface("peers")}
      />
    {:else if popover.surfaceId === "map"}
      <MapSidePanel onPick={() => commitPopoverSurface("map")} />
    {:else if popover.surfaceId === "web"}
      <WebRailList onPickTab={() => commitPopoverSurface("web")} />
    {:else if popover.surfaceId === "calendar"}
      <CalendarRailList onPickEvent={() => commitPopoverSurface("calendar")} />
    {:else if popover.surfaceId === "work"}
      <WorkRailList onPickCard={() => commitPopoverSurface("work")} />
    {:else if popover.surfaceId === "profiles"}
      <YouRailList onPickProfile={() => commitPopoverSurface("profiles")} />
    {/if}
  </NavRailViewPopover>
{/if}

<style>
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
