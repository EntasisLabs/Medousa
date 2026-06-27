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
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { layoutDesktopRails } from "$lib/utils/desktopRails";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import IdentityDrawer from "$lib/components/chat/IdentityDrawer.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import ContextPanel from "$lib/components/context/ContextPanel.svelte";
  import ProfilesPanel from "$lib/components/profiles/ProfilesPanel.svelte";
  import AutomationsPanel from "$lib/components/automations/AutomationsPanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";
  import LibraryPanel from "$lib/components/vault/LibraryPanel.svelte";
  import BrowserPanel from "$lib/components/browser/BrowserPanel.svelte";
  import WorkPanel from "$lib/components/work/WorkPanel.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { isTauri, updateTrayBlockedCount } from "$lib/window";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { DaemonHealth } from "$lib/daemon";

  let daemonHealth = $state<DaemonHealth | null>(null);

  const activeSurface = $derived(layout.desktopSurface);

  $effect(() => {
    if (!isTauri()) return;
    void updateTrayBlockedCount(workspace.blockedCount());
  });

  onMount(() => {
    void workshops.load();
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

  function navigateToSurface(surface: Surface) {
    layout.navigateDesktop(surface, { bump: true });
    if (surface === "work") {
      void workspace.prefetchCardDetails();
    }
    if (surface === "chat") {
      void chat.refreshSessions();
      void chat.ensureSessionHydrated();
    }
    if (surface === "web") {
      void chat.ensureSessionHydrated();
    }
  }

  function goToSurface(surface: Surface) {
    navigateToSurface(surface);
  }

  function handleSurfaceSelect(surface: Surface) {
    navigateToSurface(surface);
  }

  async function handleOpenNote(path: string) {
    layout.navigateDesktop("library");
    await vault.openNote(path);
  }

  async function handleCardSelect(id: string) {
    layout.navigateDesktop("work");
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
      activeProfileLabel={userProfiles.activeDisplayName}
    />

    <div class="workshop-main relative flex min-w-0 flex-1 flex-col">
      <div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {#key layout.navigationEpoch}
          {#if activeSurface === "chat"}
            <ChatPanel
              visible={true}
              onOpenContext={() => {
                layout.setIdentityDrawerOpen(false);
                goToSurface("context");
              }}
              onOpenConnection={() => {
                settingsNav.openSection("models");
                goToSurface("settings");
              }}
            />
          {:else if activeSurface === "library"}
            <LibraryPanel
              visible={true}
              onOpenChat={() => goToSurface("chat")}
              onOpenWork={() => goToSurface("work")}
              onSelectCard={handleCardSelect}
            />
          {:else if activeSurface === "web"}
            <BrowserPanel visible={true} />
          {:else if activeSurface === "context"}
            <ContextPanel
              visible={true}
              onOpenChat={async (sessionId) => {
                goToSurface("chat");
                await chat.switchSession(sessionId);
              }}
            />
          {:else if activeSurface === "profiles"}
            <ProfilesPanel visible={true} onOpenChat={() => goToSurface("chat")} />
          {:else if activeSurface === "workshop"}
            <SkillsPanel
              visible={true}
              onOpenChat={() => goToSurface("chat")}
              onScheduleSkill={(entry) => {
                automationDraft.openCreate(
                  automationDraftForSpecialist(entry, catalog.manuscriptDetail),
                );
                navigateToSurface("automations");
              }}
              onUseInAutomation={(entry) => {
                automationDraft.openCreate(
                  automationDraftForSpecialist(entry, catalog.manuscriptDetail),
                );
                navigateToSurface("automations");
              }}
            />
          {:else if activeSurface === "automations"}
            <AutomationsPanel visible={true} />
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
              onOpenCron={() => navigateToSurface("automations")}
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
          {/if}
        {/key}
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
              browserUrl={browser.activeUrl}
              browserTitle={browser.scopeLabel}
              cardDetail={activeSurface === "work"
                ? null
                : workspace.selectedCardDetail}
              cardError={workspace.cardDetailError}
              noteDiffChip={vault.diffChipText}
              onOpenNote={handleOpenNote}
              onOpenWeb={() => layout.navigateDesktop("web", { bump: true })}
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
          onOpenFullContext={() => {
            layout.setIdentityDrawerOpen(false);
            goToSurface("context");
          }}
        />
      {/if}

      <StatusBar
        minimal={activeSurface === "chat"}
        continuity={activeSurface === "library"}
        health={daemonHealth}
        workshopLabel={activeSurface === "library" || workshops.hasMultipleWorkshops
          ? workshops.activeLabel
          : null}
        inMotionCount={workspace.inMotionCount()}
        needsAttentionCount={workspace.needsAttentionCount()}
        cronActiveCount={automations.activeCount().enabled}
        cronTotalCount={automations.activeCount().total}
        pendingDeliveries={runtime.delivery?.pending_job_deliveries ?? null}
        lastTickAt={runtime.stats?.last_tick_at_utc ?? null}
        onOpenRuntime={() => navigateToSurface("runtime")}
        onOpenCron={() => navigateToSurface("automations")}
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
