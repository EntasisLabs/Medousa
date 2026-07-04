<script lang="ts">
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { feedBadgeForComponents } from "$lib/utils/customViewStatus";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import { Settings, UserRound } from "@lucide/svelte";
  import type { SurfaceDef } from "$lib/types/environment";
  import {
    SAFETY_SURFACE_RUNTIME,
    SAFETY_SURFACE_SETTINGS,
  } from "$lib/types/environment";

  interface Props {
    active: string;
    onSelect: (surface: string) => void;
    chatActivity?: number;
    workActivity?: number;
    activeProfileLabel?: string;
  }

  let {
    active,
    onSelect,
    chatActivity = 0,
    workActivity = 0,
    activeProfileLabel = "Personal",
  }: Props = $props();

  const LIFE_IDS = new Set(["chat", "work", "library", "web", "context"]);
  const WORKSHOP_IDS = new Set(["workshop", "automations"]);
  const UTILITY_IDS = new Set(["messaging", SAFETY_SURFACE_RUNTIME]);

  function navTier(surface: SurfaceDef): "life" | "workshop" | "utility" | "hidden" {
    if (surface.id === "home" || surface.id === SAFETY_SURFACE_SETTINGS) return "hidden";
    if (surface.kind === "custom") return "life";
    if (WORKSHOP_IDS.has(surface.id)) return "workshop";
    if (UTILITY_IDS.has(surface.id)) return "utility";
    if (LIFE_IDS.has(surface.id)) return "life";
    return "life";
  }

  const surfaces = $derived(environment.navSurfaces());
  const lifeOrbit = $derived(surfaces.filter((surface) => navTier(surface) === "life"));
  const workshopNav = $derived(surfaces.filter((surface) => navTier(surface) === "workshop"));
  const utility = $derived(surfaces.filter((surface) => navTier(surface) === "utility"));

  const iconProps = { size: 18, strokeWidth: 1.75 };
  const utilityIconProps = { size: 16, strokeWidth: 1.75 };

  function navTitle(surface: SurfaceDef): string {
    if (surface.id === "context") return "Threads & memory";
    return surface.label;
  }

  function activityFor(id: string): number {
    if (id === "chat") return chatActivity;
    if (id === "work") return workActivity;
    return 0;
  }

  function feedBadgeForSurface(surface: SurfaceDef): "live" | "stale" | "none" {
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
</script>

<nav class="workshop-icon-rail" aria-label="Primary navigation" data-debug-label="nav-rail">
  <WorkshopSwitcherCompact variant="rail" />
  <EnvironmentPresetSwitcher variant="rail" />

  <div class="flex flex-1 flex-col gap-0.5">
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
        onclick={() => onSelect(surface.id)}
      >
        <Icon {...iconProps} />
        {#if badge > 0}
          <span class="workshop-rail-badge" aria-hidden="true"></span>
        {:else if feedBadge !== "none"}
          <span
            class="workshop-rail-feed-badge workshop-rail-feed-badge-{feedBadge}"
            aria-hidden="true"
            title={feedBadge === "live" ? "Live feed" : "Stale feed"}
          ></span>
        {/if}
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
          onclick={() => onSelect(surface.id)}
        >
          <Icon {...iconProps} />
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
          onclick={() => onSelect(surface.id)}
        >
          <Icon {...utilityIconProps} />
        </button>
      {/each}
    {/if}
  </div>

  <button
    type="button"
    class="workshop-rail-btn workshop-rail-btn-tier-utility relative mt-2 text-[10px] font-semibold uppercase tracking-wide text-surface-300 {active ===
    'profiles'
      ? 'workshop-rail-btn-active'
      : ''}"
    title="You — {activeProfileLabel}"
    aria-label="You ({activeProfileLabel})"
    aria-current={active === "profiles" ? "page" : undefined}
    onclick={() => onSelect("profiles")}
  >
    <UserRound {...utilityIconProps} />
  </button>

  <button
    type="button"
    class="workshop-rail-btn workshop-rail-btn-tier-utility relative mt-2 {active === SAFETY_SURFACE_SETTINGS
      ? 'workshop-rail-btn-active'
      : ''}"
    title="Settings"
    aria-label="Settings"
    aria-current={active === SAFETY_SURFACE_SETTINGS ? "page" : undefined}
    onclick={() => onSelect(SAFETY_SURFACE_SETTINGS)}
  >
    <Settings {...utilityIconProps} />
  </button>
</nav>
