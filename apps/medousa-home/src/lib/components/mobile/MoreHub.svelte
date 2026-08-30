<script lang="ts">
  import MapPanel from "$lib/components/context/MapPanel.svelte";
  import ProfilesPanel from "$lib/components/profiles/ProfilesPanel.svelte";
  import AutomationsPanel from "$lib/components/automations/AutomationsPanel.svelte";
  import CalendarPanel from "$lib/components/calendar/CalendarPanel.svelte";
  import MobileCodePanel from "$lib/components/mobile/MobileCodePanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import PeersPanel from "$lib/components/peers/PeersPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";
  import { layout } from "$lib/runtime/layout.svelte";
  import { MORE_DESTINATIONS, type MoreDestination } from "$lib/types/mobile";
  import type { DaemonHealth } from "$lib/daemon";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    revision: number;
    onOpenChat: (sessionId?: string) => void | Promise<void>;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, health, revision, onOpenChat, onDaemonHealth }: Props = $props();

  const destinationById = $derived(
    Object.fromEntries(MORE_DESTINATIONS.map((dest) => [dest.id, dest])) as Record<
      Exclude<MoreDestination, "hub">,
      { id: Exclude<MoreDestination, "hub">; label: string; hint: string }
    >,
  );

  const activeLabel = $derived(
    destinationById[layout.moreDestination as Exclude<MoreDestination, "hub">]?.label ??
      layout.moreDestination,
  );

  /** Destinations without their own embedded page title. */
  const showDestTitle = $derived(
    layout.moreDestination === "map" ||
      layout.moreDestination === "profiles" ||
      layout.moreDestination === "messaging",
  );

  // Hub list removed — bounce to Home if we land on an empty more host.
  $effect(() => {
    if (!visible) return;
    if (layout.moreDestination === "hub") {
      layout.setMobileTab("home", { bump: true });
    }
  });
</script>

<div class="flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  {#if layout.moreDestination !== "hub"}
    {#if showDestTitle}
      <header class="shrink-0 px-4 pb-2 pt-3">
        <h1 class="text-lg font-semibold tracking-tight text-surface-50">{activeLabel}</h1>
      </header>
    {/if}
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
      {:else if layout.moreDestination === "map"}
        <MapPanel visible={true} />
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
        <AutomationsPanel visible={true} embedded={true} mobile={true} {onOpenChat} />
      {:else if layout.moreDestination === "code"}
        <MobileCodePanel />
      {:else if layout.moreDestination === "calendar"}
        <CalendarPanel visible={true} embedded={true} mobile={true} />
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
