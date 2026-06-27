<script lang="ts">
  import {
    Activity,
    BookOpen,
    Calendar,
    ChevronLeft,
    ChevronRight,
    Orbit,
    Radio,
    Settings,
    Sparkles,
    UserRound,
  } from "@lucide/svelte";
  import ContextPanel from "$lib/components/context/ContextPanel.svelte";
  import ProfilesPanel from "$lib/components/profiles/ProfilesPanel.svelte";
  import AutomationsPanel from "$lib/components/automations/AutomationsPanel.svelte";
  import MobileLibraryPanel from "$lib/components/mobile/MobileLibraryPanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";
  import { layout } from "$lib/stores/layout.svelte";
  import {
    YOU_DESTINATIONS,
    YOU_HUB_SECTIONS,
    type YouDestination,
  } from "$lib/types/mobile";
  import type { DaemonHealth } from "$lib/daemon";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { Globe } from "@lucide/svelte";
  import BrowserPanel from "$lib/components/browser/BrowserPanel.svelte";
  import type { Component } from "svelte";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    revision: number;
    onOpenChat: (sessionId?: string) => void | Promise<void>;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, health, revision, onOpenChat, onDaemonHealth }: Props = $props();

  const destinationIcons: Record<Exclude<YouDestination, "hub">, Component> = {
    profiles: UserRound,
    library: BookOpen,
    web: Globe,
    context: Orbit,
    workshop: Sparkles,
    automations: Calendar,
    messaging: Radio,
    settings: Settings,
    runtime: Activity,
  };

  const destinationById = $derived(
    Object.fromEntries(YOU_DESTINATIONS.map((dest) => [dest.id, dest])) as Record<
      Exclude<YouDestination, "hub">,
      (typeof YOU_DESTINATIONS)[number]
    >,
  );

  function openDestination(id: Exclude<YouDestination, "hub">) {
    layout.openYou(id);
  }

  const activeLabel = $derived(
    YOU_DESTINATIONS.find((dest) => dest.id === layout.youDestination)?.label ??
      layout.youDestination,
  );
</script>

<div class="flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  {#if layout.youDestination === "hub"}
    <header class="mobile-you-header">
      <h1 class="text-lg font-semibold tracking-tight text-surface-50">You</h1>
      <p class="workshop-faint mt-1 text-sm">Settings & tools — only when you need them</p>
    </header>
    <div class="mobile-you-scroll flex-1 overflow-y-auto px-4 pb-4">
      {#each YOU_HUB_SECTIONS as section (section.title)}
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
    <header class="mobile-you-subheader flex items-center gap-2">
      <button
        type="button"
        class="mobile-icon-btn shrink-0"
        aria-label="Back to You hub"
        onclick={() => layout.backToYouHub()}
      >
        <ChevronLeft size={20} strokeWidth={1.75} />
      </button>
      <h1 class="min-w-0 truncate text-sm font-semibold">{activeLabel}</h1>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden">
      {#if layout.youDestination === "profiles"}
        <ProfilesPanel
          visible={true}
          embedded={true}
          mobile={true}
          onOpenChat={async () => {
            await onOpenChat();
          }}
        />
      {:else if layout.youDestination === "library"}
        <MobileLibraryPanel
          visible={true}
          onOpenChat={async () => {
            await onOpenChat();
          }}
        />
      {:else if layout.youDestination === "web"}
        <BrowserPanel visible={true} mobile={true} />
      {:else if layout.youDestination === "context"}
        <ContextPanel
          visible={true}
          embedded={true}
          mobile={true}
          onOpenChat={async (sessionId) => {
            await onOpenChat(sessionId);
          }}
        />
      {:else if layout.youDestination === "workshop"}
        <SkillsPanel
          visible={true}
          embedded={true}
          mobile={true}
          {onOpenChat}
          onScheduleSkill={(entry) => {
            automationDraft.openCreate(
              automationDraftForSpecialist(entry, catalog.manuscriptDetail),
            );
            layout.openYou("automations");
          }}
          onUseInAutomation={(entry) => {
            automationDraft.openCreate(
              automationDraftForSpecialist(entry, catalog.manuscriptDetail),
            );
            layout.openYou("automations");
          }}
        />
      {:else if layout.youDestination === "automations"}
        <AutomationsPanel visible={true} embedded={true} mobile={true} />
      {:else if layout.youDestination === "messaging"}
        <MessagingPanel visible={true} {health} embedded={true} mobile={true} />
      {:else if layout.youDestination === "settings"}
        <SettingsPanel
          visible={true}
          embedded={true}
          mobile={true}
          {revision}
          {health}
          {onDaemonHealth}
        />
      {:else if layout.youDestination === "runtime"}
        <RuntimePanel
          visible={true}
          embedded={true}
          mobile={true}
          inMotionCount={workspace.inMotionCount()}
          onOpenCron={() => layout.openYou("automations")}
        />
      {/if}
    </div>
  {/if}
</div>
