<script lang="ts">
  import {
    Activity,
    Calendar,
    ChevronLeft,
    ChevronRight,
    Orbit,
    Radio,
    Settings,
    Sparkles,
    UserRound,
    Users,
  } from "@lucide/svelte";
  import ContextPanel from "$lib/components/context/ContextPanel.svelte";
  import ProfilesPanel from "$lib/components/profiles/ProfilesPanel.svelte";
  import AutomationsPanel from "$lib/components/automations/AutomationsPanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import PeersPanel from "$lib/components/peers/PeersPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";
  import { layout } from "$lib/stores/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    MORE_DESTINATIONS,
    MORE_HUB_SECTIONS,
    type MoreDestination,
  } from "$lib/types/mobile";
  import type { DaemonHealth } from "$lib/daemon";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { Component } from "svelte";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    revision: number;
    onOpenChat: (sessionId?: string) => void | Promise<void>;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, health, revision, onOpenChat, onDaemonHealth }: Props = $props();

  const destinationIcons: Record<Exclude<MoreDestination, "hub">, Component> = {
    profiles: UserRound,
    context: Orbit,
    workshop: Sparkles,
    automations: Calendar,
    messaging: Radio,
    peers: Users,
    settings: Settings,
    runtime: Activity,
  };

  const destinationById = $derived(
    Object.fromEntries(
      [
        ...MORE_DESTINATIONS,
        {
          id: "automations" as const,
          label: "Automations",
          hint: "Scripts, flows, schedules & history",
        },
      ].map((dest) => [dest.id, dest]),
    ) as Record<
      Exclude<MoreDestination, "hub">,
      { id: Exclude<MoreDestination, "hub">; label: string; hint: string }
    >,
  );

  function openDestination(id: Exclude<MoreDestination, "hub">) {
    layout.openMore(id);
  }

  const activeLabel = $derived(
    destinationById[layout.moreDestination as Exclude<MoreDestination, "hub">]?.label ??
      layout.moreDestination,
  );

  const myCustomViews = $derived(
    environment.navSurfaces().filter((surface) => surface.kind === "custom"),
  );

  function openCustomView(surfaceId: string) {
    layout.openCustomSurface(surfaceId);
  }
</script>

<div class="flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  {#if layout.moreDestination === "hub"}
    <header class="mobile-you-header">
      <h1 class="text-lg font-semibold tracking-tight text-surface-50">More</h1>
      <p class="workshop-faint mt-1 text-sm">Settings & tools — when you need them</p>
    </header>
    <div class="mobile-you-scroll flex-1 overflow-y-auto px-4 pb-4">
      {#if myCustomViews.length > 0}
        <section class="mb-6">
          <h2 class="mobile-you-section-title">My views</h2>
          <p class="workshop-faint mb-2 text-xs">Custom surfaces from your active layout preset</p>
          <ul class="space-y-2">
            {#each myCustomViews as surface (surface.id)}
              {@const SurfaceIcon = environmentIcon(surface.icon)}
              <li>
                <button
                  type="button"
                  class="mobile-you-destination"
                  onclick={() => openCustomView(surface.id)}
                >
                  <span class="mobile-you-destination-icon">
                    <SurfaceIcon size={18} strokeWidth={1.75} />
                  </span>
                  <span class="min-w-0 flex-1">
                    <p class="font-medium text-surface-100">{surface.label}</p>
                    <p class="workshop-faint mt-0.5 text-xs">{surface.id}</p>
                  </span>
                  <ChevronRight size={16} class="shrink-0 text-surface-600" />
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
      {#each MORE_HUB_SECTIONS as section (section.title)}
        <section class="mb-6">
          <h2 class="mobile-you-section-title">{section.title}</h2>
          <p class="workshop-faint mb-2 text-xs">{section.subtitle}</p>
          <ul class="space-y-2">
            {#each section.destinations as destId (destId)}
              {@const dest = destinationById[destId]}
              {@const Icon = destinationIcons[destId]}
              <li>
                <button
                  type="button"
                  class="mobile-you-destination"
                  onclick={() => openDestination(destId)}
                >
                  <span class="mobile-you-destination-icon">
                    <Icon size={18} strokeWidth={1.75} />
                  </span>
                  <span class="min-w-0 flex-1">
                    <p class="font-medium text-surface-100">{dest.label}</p>
                    <p class="workshop-faint mt-0.5 text-xs">{dest.hint}</p>
                  </span>
                  <ChevronRight size={16} class="shrink-0 text-surface-600" />
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {:else}
    <header class="mobile-you-subheader gap-2">
      <button
        type="button"
        class="mobile-icon-btn shrink-0"
        aria-label="Back to More hub"
        onclick={() => layout.backToMoreHub()}
      >
        <ChevronLeft size={20} strokeWidth={1.75} />
      </button>
      <h1 class="min-w-0 truncate text-sm font-semibold">{activeLabel}</h1>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden">
      {#if layout.moreDestination === "profiles"}
        <ProfilesPanel
          visible={true}
          embedded={true}
          mobile={true}
          onOpenChat={async () => {
            await onOpenChat();
          }}
        />
      {:else if layout.moreDestination === "context"}
        <ContextPanel
          visible={true}
          embedded={true}
          mobile={true}
          onOpenChat={async (sessionId) => {
            await onOpenChat(sessionId);
          }}
        />
      {:else if layout.moreDestination === "workshop"}
        <SkillsPanel
          visible={true}
          embedded={true}
          mobile={true}
          {onOpenChat}
          onScheduleSkill={(entry) => {
            automationDraft.openCreate(
              automationDraftForSpecialist(entry, catalog.manuscriptDetail),
            );
            layout.openMore("automations");
          }}
          onUseInAutomation={(entry) => {
            automationDraft.openCreate(
              automationDraftForSpecialist(entry, catalog.manuscriptDetail),
            );
            layout.openMore("automations");
          }}
        />
      {:else if layout.moreDestination === "automations"}
        <AutomationsPanel visible={true} embedded={true} mobile={true} />
      {:else if layout.moreDestination === "messaging"}
        <MessagingPanel visible={true} {health} embedded={true} mobile={true} />
      {:else if layout.moreDestination === "peers"}
        <PeersPanel visible={true} embedded={true} mobile={true} />
      {:else if layout.moreDestination === "settings"}
        <SettingsPanel
          visible={true}
          embedded={true}
          mobile={true}
          {revision}
          {health}
          {onDaemonHealth}
        />
      {:else if layout.moreDestination === "runtime"}
        <RuntimePanel
          visible={true}
          embedded={true}
          mobile={true}
          inMotionCount={workspace.inMotionCount()}
        />
      {/if}
    </div>
  {/if}
</div>
