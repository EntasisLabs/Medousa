<script lang="ts">
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
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
  import { ChevronLeft, PanelLeftClose, Settings, UserRound } from "@lucide/svelte";
  import { SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";
  import type { DaemonHealth } from "$lib/daemon";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";

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
  const utilityIconProps = { size: 16, strokeWidth: 1.5 };

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

  function railBtnClass(id: string, tier: "life" | "utility"): string {
    const isActive = active === id;
    const activeClass = isActive ? "workshop-rail-btn-active" : "";
    const tierClass =
      tier === "life" ? "workshop-rail-btn-tier-life" : "workshop-rail-btn-tier-utility";
    return `workshop-rail-btn relative ${tierClass} ${activeClass}`;
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

        <div class="workshop-icon-rail-items flex flex-1 flex-col gap-1">
          {#each lifeOrbit as surface (surface.id)}
            {@const Icon = environmentIcon(surface.icon)}
            {@const badge = activityFor(surface.id)}
            {@const feedBadge = feedBadgeForSurface(surface)}
            <button
              type="button"
              class={railBtnClass(surface.id, "life")}
              title={navTitle(surface)}
              aria-label={badge > 0 ? `${navTitle(surface)} (${badge} active)` : navTitle(surface)}
              aria-current={active === surface.id ? "page" : undefined}
              onclick={() => selectDestination(surface.id)}
            >
              <span class="workshop-rail-btn-icon" aria-hidden="true">
                <Icon {...iconProps} />
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
                class={railBtnClass(surface.id, "utility")}
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

        <button
          type="button"
          class="workshop-rail-btn workshop-rail-btn-tier-utility relative mt-2 {active ===
          'profiles'
            ? 'workshop-rail-btn-active'
            : ''}"
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
          class="workshop-rail-btn workshop-rail-btn-tier-utility relative mt-1 mb-1 {active ===
          SAFETY_SURFACE_SETTINGS
            ? 'workshop-rail-btn-active'
            : ''}"
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
    padding: 0.2rem 0.15rem;
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
