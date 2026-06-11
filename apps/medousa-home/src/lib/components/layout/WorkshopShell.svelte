<script lang="ts">
  import { onMount } from "svelte";
  import NavSidebar from "$lib/components/layout/NavSidebar.svelte";
  import { connectWorkshop, refreshDaemonHealth } from "$lib/workshopConnection";
  import ActivityCollapsedStrip from "$lib/components/layout/ActivityCollapsedStrip.svelte";
  import type { Surface } from "$lib/types/ui";
  import WorkRail from "$lib/components/layout/WorkRail.svelte";
  import ActivityPanel from "$lib/components/layout/ActivityPanel.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import { layout, loadLastSurface, saveLastSurface } from "$lib/stores/layout.svelte";
  import { layoutDesktopRails } from "$lib/utils/desktopRails";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import IdentityDrawer from "$lib/components/chat/IdentityDrawer.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import CronPanel from "$lib/components/cron/CronPanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { cronDraft } from "$lib/stores/cron.svelte";
  import LibraryPanel from "$lib/components/vault/LibraryPanel.svelte";
  import WorkPanel from "$lib/components/work/WorkPanel.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { isTauri, updateTrayBlockedCount } from "$lib/window";
  import type { DaemonHealth } from "$lib/daemon";

  let activeSurface = $state<Surface>(loadLastSurface());
  let daemonHealth = $state<DaemonHealth | null>(null);

  $effect(() => {
    if (!isTauri()) return;
    void updateTrayBlockedCount(workspace.blockedCount());
  });

  onMount(() => {
    const detachViewport = layout.attachViewportTracking();
    const detachWorkshop = connectWorkshop({
      onHealthChange: (health) => {
        daemonHealth = health;
      },
    });
    return () => {
      detachViewport();
      detachWorkshop();
    };
  });

  const desktopRails = $derived(
    layoutDesktopRails({
      viewportWidth: layout.viewportWidth,
      activityCollapsed: layout.activityCollapsed,
      activityWidth: layout.activityWidth,
      workInspectorOpen: false,
      workInspectorWidth: layout.workInspectorWidth,
    }),
  );

  function goToSurface(surface: Surface) {
    activeSurface = surface;
    saveLastSurface(surface);
    if (surface === "chat") {
      void chat.refreshSessions();
      void chat.ensureSessionHydrated();
    }
    if (surface === "work") {
      layout.setActivityCollapsed(true);
      void workspace.prefetchCardDetails();
    }
  }

  function handleSurfaceSelect(surface: Surface) {
    if (surface === "work") {
      layout.setActivityCollapsed(true);
    }
    activeSurface = surface;
    saveLastSurface(surface);
    if (surface === "chat") {
      void chat.refreshSessions();
      void chat.ensureSessionHydrated();
    }
    if (surface === "work") {
      void workspace.prefetchCardDetails();
    }
  }

  async function handleOpenNote(path: string) {
    activeSurface = "library";
    saveLastSurface("library");
    await vault.openNote(path);
  }

  async function handleCardSelect(id: string) {
    activeSurface = "work";
    saveLastSurface("work");
    layout.setActivityCollapsed(true);
    await workspace.selectCard(id);
  }
</script>

<div class="flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <div class="flex min-h-0 flex-1">
    <NavSidebar
      active={activeSurface}
      onSelect={handleSurfaceSelect}
      chatActivity={chat.backgroundActivity}
      workActivity={workspace.inMotionCount()}
    />

    <div class="workshop-main relative flex min-w-0 flex-1 flex-col">
      <div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {#if activeSurface === "library"}
          <LibraryPanel
            visible={true}
            onOpenChat={() => goToSurface("chat")}
            onOpenWork={() => goToSurface("work")}
            onSelectCard={handleCardSelect}
          />
        {:else if activeSurface === "skills"}
          <SkillsPanel
            visible={true}
            onOpenChat={() => goToSurface("chat")}
            onScheduleSkill={(entry) => {
              cronDraft.openCreate({
                prompt: `Run ${entry.name} on schedule`,
                cron_expr: "0 9 * * *",
                manuscript_id: entry.id,
              });
              activeSurface = "cron";
            }}
          />
        {:else if activeSurface === "cron"}
          <CronPanel visible={true} />
        {:else if activeSurface === "messaging"}
          <MessagingPanel visible={true} health={daemonHealth} />
        {:else if activeSurface === "work"}
          <WorkPanel
            visible={true}
            onOpenNote={handleOpenNote}
            onOpenChat={() => goToSurface("chat")}
            onSelectCard={handleCardSelect}
          />
        {:else if activeSurface === "runtime"}
          <RuntimePanel
            visible={true}
            inMotionCount={workspace.inMotionCount()}
            onOpenCron={() => (activeSurface = "cron")}
          />
        {:else if activeSurface === "settings"}
          <SettingsPanel
            visible={true}
            revision={workspace.revision}
            health={daemonHealth}
            onDaemonHealth={async () => {
              daemonHealth = await refreshDaemonHealth();
            }}
          />
        {:else}
          <ChatPanel visible={activeSurface === "chat"} />
        {/if}
        </div>

        {#if layout.activityCollapsed || desktopRails.showActivityStrip}
          <ActivityCollapsedStrip
            onExpand={() => layout.setActivityCollapsed(false)}
          />
        {:else}
          <div class="workshop-rail flex h-full min-w-0 shrink-0 overflow-hidden">
          <SplitPane
            width={desktopRails.activityPaneWidth}
            side="right"
            min={220}
            max={desktopRails.activityPaneMax}
            onResize={(width) => layout.setActivityWidth(width)}
          >
            <ActivityPanel
              events={workspace.feed}
              error={workspace.streamError}
              notePath={vault.selectedPath}
              noteTitle={vault.title}
              wikilinksOut={vault.wikilinksOut}
              backlinks={vault.backlinks}
              cardDetail={activeSurface === "work"
                ? null
                : workspace.selectedCardDetail}
              cardError={workspace.cardDetailError}
              noteDiffChip={vault.diffChip()}
              onOpenNote={handleOpenNote}
              onSelectCard={handleCardSelect}
              onCollapse={() => layout.setActivityCollapsed(true)}
            />
          </SplitPane>
          </div>
        {/if}
      </div>

      {#if activeSurface === "chat"}
        <SessionSidebar
          open={layout.sessionDrawerOpen}
          onClose={() => layout.setSessionDrawerOpen(false)}
        />
        <IdentityDrawer
          open={layout.identityDrawerOpen}
          onClose={() => layout.setIdentityDrawerOpen(false)}
        />
      {/if}

      <StatusBar
        minimal={activeSurface === "chat"}
        health={daemonHealth}
        inMotionCount={workspace.inMotionCount()}
        needsAttentionCount={workspace.needsAttentionCount()}
        cronActiveCount={recurring.activeCount().enabled}
        cronTotalCount={recurring.activeCount().total}
        pendingDeliveries={runtime.delivery?.pending_job_deliveries ?? null}
        lastTickAt={runtime.stats?.last_tick_at_utc ?? null}
        onOpenRuntime={() => (activeSurface = "runtime")}
        onOpenCron={() => (activeSurface = "cron")}
      />

      {#if workspace.inMotionCount() > 0 && activeSurface !== "work"}
        <WorkRail
          cards={workspace.railCards()}
          selectedId={workspace.selectedCardId}
          onSelect={handleCardSelect}
        />
      {/if}
    </div>
  </div>
</div>
