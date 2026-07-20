<script lang="ts">
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import ContextSidePanel from "$lib/components/context/ContextSidePanel.svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import LmeSidePanel from "$lib/components/lme/LmeSidePanel.svelte";
  import MessagingChannelList from "$lib/components/messaging/MessagingChannelList.svelte";
  import PeersShellList from "$lib/components/peers/PeersShellList.svelte";
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { messaging } from "$lib/stores/messaging.svelte";
  import { messagingShell } from "$lib/stores/messagingShell.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { feedBadgeForComponents } from "$lib/utils/customViewStatus";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    navLabel,
    navTier,
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
  import { ChevronLeft, ChevronRight, PanelLeftClose, Settings, UserRound } from "@lucide/svelte";
  import { SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";
  import type { DaemonHealth } from "$lib/daemon";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { onMount } from "svelte";

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
  /** View list fills the same rail — never a second column. */
  const showView = $derived(mode === "view" && surfaceHasShellSidebarView(active));
  const viewTitle = $derived(shellSidebarViewTitle(active));
  const daemonOk = $derived(health?.ok ?? false);

  const surfaces = $derived(environment.navSurfaces());
  const lifeOrbit = $derived(surfaces.filter((surface) => navTier(surface) === "life"));
  const workshopNav = $derived(surfaces.filter((surface) => navTier(surface) === "workshop"));
  const utility = $derived(surfaces.filter((surface) => navTier(surface) === "utility"));

  const iconProps = { size: 18, strokeWidth: 1.75 };
  /** Quieter tree parents — closer to Cursor folder icons. */
  const treeIconProps = { size: 14, strokeWidth: 1.5 };
  const utilityIconProps = { size: 14, strokeWidth: 1.5 };
  /** Explicit open map; missing key = collapsed by default. */
  let nestOpen = $state<Record<string, boolean>>(loadNestOpen());

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
    options?: { quietActive?: boolean },
  ): string {
    const isActive = active === id;
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

  function hideRail() {
    layout.setShellSidebarExpanded(false);
    void environment.patchShellChromeDesktop({ navStyle: "compact" }).catch(() => {});
  }

  function selectDestination(surfaceId: string) {
    onSelect(surfaceId);
    if (surfaceHasShellSidebarView(surfaceId)) {
      layout.setShellSidebarMode("view");
    } else {
      layout.setShellSidebarMode("nav");
    }
  }

  function backToNav() {
    layout.shellSidebarBackToNav();
  }

  function nestFor(surfaceId: string): NavRailNestItem[] {
    if (!surfaceSupportsRailNest(surfaceId)) return [];
    return nestItemsForSurface(surfaceId);
  }

  function isNestExpanded(surfaceId: string): boolean {
    return nestOpen[surfaceId] === true;
  }

  function persistNestOpen() {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(NEST_OPEN_KEY, JSON.stringify(nestOpen));
  }

  function toggleNest(surfaceId: string, event: Event) {
    event.preventDefault();
    event.stopPropagation();
    nestOpen = { ...nestOpen, [surfaceId]: !isNestExpanded(surfaceId) };
    persistNestOpen();
  }

  async function openNestItem(surfaceId: string, item: NavRailNestItem) {
    onSelect(surfaceId);
    // Keep hierarchical nav visible (Cursor-style); don’t morph into view mode.
    layout.setShellSidebarMode("nav");
    if (!isNestExpanded(surfaceId)) {
      nestOpen = { ...nestOpen, [surfaceId]: true };
      persistNestOpen();
    }
    await activateNestItem(surfaceId, item.id);
  }

  onMount(() => {
    prefetchRailNestData();
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
          {#if active === SAFETY_SURFACE_SETTINGS}
            <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1">
              <SettingsNav
                active={settingsNav.activeSection}
                onSelect={(section) => settingsNav.setActiveSection(section)}
              />
            </div>
          {:else if active === "chat"}
            <SessionSidebar open={true} variant="inline" />
          {:else if active === "library" || active === "automations"}
            <LmeSidePanel {onOpenChat} />
          {:else if active === "messaging"}
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
          {:else if active === "peers"}
            <PeersShellList />
          {:else if active === "context"}
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
          class="workshop-icon-rail-items workshop-rail-tree flex min-h-0 flex-1 flex-col overflow-y-auto"
        >
          {#each lifeOrbit as surface (surface.id)}
            {@const Icon = environmentIcon(surface.icon)}
            {@const badge = activityFor(surface.id)}
            {@const feedBadge = feedBadgeForSurface(surface)}
            {@const nest = nestFor(surface.id)}
            {@const leafActive = nestHasActiveItem(surface.id, nest)}
            {@const nestExpanded = nest.length > 0 && isNestExpanded(surface.id)}
            <div
              class="workshop-rail-dest"
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
                  <span class="workshop-rail-dest-twist workshop-rail-dest-twist-empty" aria-hidden="true"
                  ></span>
                {/if}
                <button
                  type="button"
                  class="{railBtnClass(surface.id, 'life', {
                    quietActive: true,
                  })} workshop-rail-dest-btn"
                  class:workshop-rail-dest-btn-dimmed={leafActive && nestExpanded}
                  title={navTitle(surface)}
                  aria-label={badge > 0 ? `${navTitle(surface)} (${badge} active)` : navTitle(surface)}
                  aria-current={active === surface.id && !leafActive ? "page" : undefined}
                  onclick={() => selectDestination(surface.id)}
                >
                  <span class="workshop-rail-btn-icon" aria-hidden="true">
                    <Icon {...treeIconProps} />
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
                  {#each nest as item (item.id)}
                    <li>
                      <button
                        type="button"
                        class="workshop-rail-nest-btn"
                        class:workshop-rail-nest-btn-active={nestItemIsActive(surface.id, item.id)}
                        class:workshop-rail-nest-btn-accent={item.accent}
                        title={item.meta ? `${item.label} · ${item.meta}` : item.label}
                        onclick={() => void openNestItem(surface.id, item)}
                      >
                        {#if item.accent}
                          <span class="workshop-rail-nest-dot" aria-hidden="true"></span>
                        {/if}
                        <span class="workshop-rail-nest-label">{item.label}</span>
                        {#if item.meta}
                          <span class="workshop-rail-nest-meta">{item.meta}</span>
                        {/if}
                      </button>
                    </li>
                  {/each}
                  {#if nest.length >= NAV_RAIL_NEST_LIMIT}
                    <li>
                      <button
                        type="button"
                        class="workshop-rail-nest-more"
                        onclick={() => selectDestination(surface.id)}
                      >
                        More
                      </button>
                    </li>
                  {/if}
                </ul>
              {/if}
            </div>
          {/each}

          {#if workshopNav.length > 0}
            <div class="workshop-rail-tier-divider" aria-hidden="true"></div>
            {#each workshopNav as surface (surface.id)}
              {@const Icon = environmentIcon(surface.icon)}
              <button
                type="button"
                class={railBtnClass(surface.id, "life")}
                title={surface.label}
                aria-label={surface.label}
                aria-current={active === surface.id ? "page" : undefined}
                onclick={() => selectDestination(surface.id)}
              >
                <span class="workshop-rail-btn-icon" aria-hidden="true">
                  <Icon {...iconProps} />
                </span>
                <span class="workshop-rail-btn-label">{surface.label}</span>
              </button>
            {/each}
          {/if}

          {#if utility.length > 0}
            <div class="workshop-rail-tier-divider" aria-hidden="true"></div>
            {#each utility as surface (surface.id)}
              {@const Icon = environmentIcon(surface.icon)}
              <button
                type="button"
                class="{railBtnClass(surface.id, 'utility', { quietActive: true })} workshop-rail-tree-row"
                title={surface.label}
                aria-label={surface.label}
                aria-current={active === surface.id ? "page" : undefined}
                onclick={() => selectDestination(surface.id)}
              >
                <span class="workshop-rail-btn-icon" aria-hidden="true">
                  <Icon {...utilityIconProps} />
                </span>
                <span class="workshop-rail-btn-label">{surface.label}</span>
              </button>
            {/each}
          {/if}
        </div>

        <div class="workshop-rail-dock">
          <button
            type="button"
            class="{railBtnClass('profiles', 'utility', { quietActive: true })} workshop-rail-dock-btn"
            title="You — {activeProfileLabel}"
            aria-label="You ({activeProfileLabel})"
            aria-current={active === "profiles" ? "page" : undefined}
            onclick={() => selectDestination("profiles")}
          >
            <span class="workshop-rail-btn-icon" aria-hidden="true">
              <UserRound {...utilityIconProps} />
            </span>
            <span class="workshop-rail-btn-label">You</span>
          </button>

          <EnvironmentPresetSwitcher variant="rail" expanded={true} />
          <WorkshopSwitcherCompact variant="rail" expanded={true} />

          <button
            type="button"
            class="{railBtnClass(SAFETY_SURFACE_SETTINGS, 'utility', {
              quietActive: true,
            })} workshop-rail-dock-btn"
            title="Settings"
            aria-label="Settings"
            aria-current={active === SAFETY_SURFACE_SETTINGS ? "page" : undefined}
            onclick={() => selectDestination(SAFETY_SURFACE_SETTINGS)}
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

<style>
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
