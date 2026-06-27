<script lang="ts">
  import {
    Activity,
    BookOpen,
    Calendar,
    Globe,
    LayoutGrid,
    MessageCircle,
    Orbit,
    Radio,
    Settings,
    UserRound,
    Zap,
  } from "@lucide/svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import type { Component } from "svelte";
  import type { Surface } from "$lib/types/ui";

  interface NavItem {
    id: Surface;
    label: string;
    icon: Component;
  }

  interface Props {
    active: Surface;
    onSelect: (surface: Surface) => void;
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

  const lifeOrbit: NavItem[] = [
    { id: "chat", label: "Chat", icon: MessageCircle },
    { id: "work", label: "Work", icon: LayoutGrid },
    { id: "library", label: "Library", icon: BookOpen },
    { id: "web", label: "Web", icon: Globe },
    { id: "context", label: "Context", icon: Orbit },
  ];

  function navTitle(item: NavItem): string {
    if (item.id === "context") return "Threads & memory";
    return item.label;
  }

  const workshopNav: NavItem[] = [
    { id: "workshop", label: "Capabilities", icon: Zap },
    { id: "automations", label: "Automations", icon: Calendar },
  ];

  const utility: NavItem[] = [
    { id: "messaging", label: "Messaging", icon: Radio },
    { id: "runtime", label: "Runtime", icon: Activity },
  ];

  const iconProps = { size: 18, strokeWidth: 1.75 };
  const utilityIconProps = { size: 16, strokeWidth: 1.75 };

  function activityFor(id: Surface): number {
    if (id === "chat") return chatActivity;
    if (id === "work") return workActivity;
    return 0;
  }

  function railBtnClass(id: Surface, tier: "life" | "utility"): string {
    const isActive = active === id;
    const activeClass = isActive ? "workshop-rail-btn-active" : "";
    const tierClass =
      tier === "life" ? "workshop-rail-btn-tier-life" : "workshop-rail-btn-tier-utility";
    return `workshop-rail-btn relative ${tierClass} ${activeClass}`;
  }
</script>

<nav class="workshop-icon-rail" aria-label="Primary navigation">
  <WorkshopSwitcherCompact variant="rail" />

  <div class="flex flex-1 flex-col gap-0.5">
    {#each lifeOrbit as item (item.id)}
      {@const Icon = item.icon}
      {@const badge = activityFor(item.id)}
      <button
        type="button"
        class={railBtnClass(item.id, "life")}
        title={navTitle(item)}
        aria-label={badge > 0 ? `${navTitle(item)} (${badge} active)` : navTitle(item)}
        aria-current={active === item.id ? "page" : undefined}
        onclick={() => onSelect(item.id)}
      >
        <Icon {...iconProps} />
        {#if badge > 0}
          <span class="workshop-rail-badge" aria-hidden="true"></span>
        {/if}
      </button>
    {/each}

    <div class="workshop-rail-tier-divider" aria-hidden="true"></div>

    {#each workshopNav as item (item.id)}
      {@const Icon = item.icon}
      <button
        type="button"
        class={railBtnClass(item.id, "life")}
        title={item.label}
        aria-label={item.label}
        aria-current={active === item.id ? "page" : undefined}
        onclick={() => onSelect(item.id)}
      >
        <Icon {...iconProps} />
      </button>
    {/each}

    <div class="workshop-rail-tier-divider" aria-hidden="true"></div>

    {#each utility as item (item.id)}
      {@const Icon = item.icon}
      <button
        type="button"
        class={railBtnClass(item.id, "utility")}
        title={item.label}
        aria-label={item.label}
        aria-current={active === item.id ? "page" : undefined}
        onclick={() => onSelect(item.id)}
      >
        <Icon {...utilityIconProps} />
      </button>
    {/each}
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
    class="workshop-rail-btn workshop-rail-btn-tier-utility relative mt-2 {active === 'settings'
      ? 'workshop-rail-btn-active'
      : ''}"
    title="Settings"
    aria-label="Settings"
    aria-current={active === "settings" ? "page" : undefined}
    onclick={() => onSelect("settings")}
  >
    <Settings {...utilityIconProps} />
  </button>
</nav>
