<script lang="ts">
  import { onMount } from "svelte";
  import NavSidebar from "$lib/components/layout/NavSidebar.svelte";
  import ActivityCollapsedStrip from "$lib/components/layout/ActivityCollapsedStrip.svelte";
  import type { Surface } from "$lib/types/ui";
  import WorkRail from "$lib/components/layout/WorkRail.svelte";
  import ActivityPanel from "$lib/components/layout/ActivityPanel.svelte";
  import HomeOverview from "$lib/components/layout/HomeOverview.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import IdentityDrawer from "$lib/components/chat/IdentityDrawer.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import CronPanel from "$lib/components/cron/CronPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { cronDraft } from "$lib/stores/cron.svelte";
  import LibraryPanel from "$lib/components/vault/LibraryPanel.svelte";
  import WorkPanel from "$lib/components/work/WorkPanel.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { isTauri, updateTrayBlockedCount } from "$lib/window";
  import {
    checkDaemonHealth,
    onInteractiveEvent,
    onInteractiveError,
    onWorkspaceEvent,
    onWorkspaceError,
    startWorkspaceStream,
    stopInteractiveStream,
    stopWorkspaceStream,
  } from "$lib/daemon";
  import type { DaemonHealth } from "$lib/daemon";
  import type { WorkspaceStreamEvent } from "$lib/types/workspace";
  import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

  let activeSurface = $state<Surface>("chat");
  let daemonHealth = $state<DaemonHealth | null>(null);

  $effect(() => {
    if (!isTauri()) return;
    void updateTrayBlockedCount(workspace.blockedCount());
  });

  onMount(() => {
    settings.applyTheme();
    const unlisteners: Promise<() => void>[] = [];

    (async () => {
      daemonHealth = await checkDaemonHealth();

      await stopWorkspaceStream();
      await startWorkspaceStream(workspace.revision || undefined);
      await runtime.loadFromTuiDefaults();
      void runtime.refresh();
      void recurring.refresh();
      void chat.refreshSessions();
      if (chat.messages.length === 0) {
        void chat.switchSession(chat.sessionId);
      }

      unlisteners.push(
        onWorkspaceEvent<WorkspaceStreamEvent>((event) => {
          workspace.applyEvent(event);
          const kind = event.feed_event?.kind;
          if (
            kind === "vault_note_created" ||
            kind === "vault_note_updated"
          ) {
            void vault.refreshNotes();
            if (
              vault.selectedPath &&
              event.feed_event?.summary.includes(vault.selectedPath)
            ) {
              void vault.openNote(vault.selectedPath);
            }
          }
        }),
      );
      unlisteners.push(
        onWorkspaceError((message) => workspace.setError(message)),
      );
      unlisteners.push(
        onInteractiveEvent<InteractiveTurnStreamEvent>((event) => {
          chat.applyStreamEvent(event);
        }),
      );
      unlisteners.push(
        onInteractiveError((message) => chat.setError(message)),
      );
    })();

    return () => {
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
      stopWorkspaceStream();
      stopInteractiveStream();
    };
  });

  function handleSurfaceSelect(surface: Surface) {
    if (surface === "work") {
      layout.setActivityCollapsed(true);
    }
    activeSurface = surface;
    if (surface === "chat") {
      void chat.refreshSessions();
    }
    if (surface === "work") {
      void workspace.prefetchCardDetails();
    }
  }

  async function handleOpenNote(path: string) {
    activeSurface = "library";
    await vault.openNote(path);
  }

  async function handleCardSelect(id: string) {
    activeSurface = "work";
    layout.setActivityCollapsed(true);
    await workspace.selectCard(id);
  }
</script>

<div class="flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <div class="flex min-h-0 flex-1">
    <NavSidebar active={activeSurface} onSelect={handleSurfaceSelect} />

    <div class="workshop-main relative flex min-w-0 flex-1 flex-col">
      <div class="flex min-h-0 flex-1">
        <div class="flex min-h-0 min-w-0 flex-1 flex-col">
        {#if activeSurface === "home"}
          <HomeOverview
            onOpenWork={() => (activeSurface = "work")}
            onOpenChat={() => (activeSurface = "chat")}
            onOpenNote={handleOpenNote}
            onSelectCard={handleCardSelect}
          />
        {:else if activeSurface === "library"}
          <LibraryPanel visible={true} />
        {:else if activeSurface === "skills"}
          <SkillsPanel
            visible={true}
            onOpenChat={() => (activeSurface = "chat")}
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
        {:else if activeSurface === "work"}
          <WorkPanel
            visible={true}
            onOpenNote={handleOpenNote}
            onOpenChat={() => (activeSurface = "chat")}
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
            onOpenRuntime={() => (activeSurface = "runtime")}
            onDaemonHealth={async () => {
              daemonHealth = await checkDaemonHealth();
            }}
          />
        {:else}
          <ChatPanel visible={activeSurface === "chat"} />
        {/if}
        </div>

        {#if layout.activityCollapsed}
          <ActivityCollapsedStrip
            onExpand={() => layout.setActivityCollapsed(false)}
          />
        {:else}
          <div class="workshop-rail flex h-full shrink-0">
          <SplitPane
            width={layout.activityWidth}
            side="right"
            min={220}
            max={520}
            onResize={(width) => layout.setActivityWidth(width)}
          >
            <ActivityPanel
              events={workspace.feed}
              error={workspace.streamError}
              notePath={vault.selectedPath}
              noteTitle={vault.title}
              wikilinksOut={vault.wikilinksOut}
              backlinks={vault.backlinks}
              cardDetail={workspace.selectedCardDetail}
              cardError={workspace.cardDetailError}
              noteDiffChip={vault.diffChip()}
              onOpenNote={handleOpenNote}
              showCollapse={activeSurface === "work"}
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

      {#if workspace.inMotionCount() > 0 || activeSurface === "work"}
        <WorkRail
          cards={workspace.railCards()}
          selectedId={workspace.selectedCardId}
          onSelect={handleCardSelect}
        />
      {/if}
    </div>
  </div>
</div>
