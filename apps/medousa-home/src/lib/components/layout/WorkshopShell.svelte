<script lang="ts">
  import { onMount } from "svelte";
  import NavSidebar from "$lib/components/layout/NavSidebar.svelte";
  import { connectWorkshop, refreshDaemonHealth } from "$lib/workshopConnection";
  import ActivityCollapsedStrip from "$lib/components/layout/ActivityCollapsedStrip.svelte";
  import EnvironmentRenderer from "$lib/components/environment/EnvironmentRenderer.svelte";
  import { environment } from "$lib/stores/environment.svelte";
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
  import PeersPanel from "$lib/components/peers/PeersPanel.svelte";
  import { peerUnreadCount } from "$lib/utils/lanShareApi";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";
  import LibraryPanel from "$lib/components/vault/LibraryPanel.svelte";
  import WorkPanel from "$lib/components/work/WorkPanel.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { browserContext } from "$lib/stores/browserContext.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { isTauri } from "$lib/platform";
  import { updateTrayBlockedCount } from "$lib/window";
  import HumanBrowserPanel from "$lib/components/browser/HumanBrowserPanel.svelte";
  import ShellLayoutDebug from "$lib/components/debug/ShellLayoutDebug.svelte";
  import EnvPendingProposalBanner from "$lib/components/environment/EnvPendingProposalBanner.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    onOpenSpotlight?: () => void;
  }

  let { onOpenSpotlight }: Props = $props();

  let daemonHealth = $state<DaemonHealth | null>(null);
  let shellRootEl = $state<HTMLElement | null>(null);
  let peersUnread = $state(0);
  let peersUnreadTimer: ReturnType<typeof setInterval> | null = null;

  const activeSurface = $derived(layout.desktopSurface);

  async function refreshPeersUnread() {
    if (!isTauri()) return;
    try {
      peersUnread = await peerUnreadCount();
    } catch {
      peersUnread = 0;
    }
  }

  $effect(() => {
    if (!isTauri()) return;
    void updateTrayBlockedCount(workspace.blockedCount());
  });

  $effect(() => {
    if (activeSurface === "peers") {
      void refreshPeersUnread();
    }
  });

  onMount(() => {
    void workshops.load();
    void refreshPeersUnread();
    peersUnreadTimer = setInterval(() => {
      void refreshPeersUnread();
    }, 8000);
    const detachViewport = layout.attachViewportTracking();
    const detachWorkshop = connectWorkshop({
      onHealthChange: (health) => {
        daemonHealth = health;
      },
    });
    const detachBrowserContext = browserContext.attachListeners();
    return () => {
      if (peersUnreadTimer) clearInterval(peersUnreadTimer);
      detachViewport();
      detachWorkshop();
      detachBrowserContext();
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

  function navigateToSurface(surface: string) {
    layout.navigateDesktop(surface, { bump: true });
    if (surface === "work") {
      void workspace.prefetchCardDetails();
    }
    if (surface === "chat") {
      void chat.refreshSessions();
      void chat.ensureSessionHydrated();
    }
  }

  function goToSurface(surface: string) {
    navigateToSurface(surface);
  }

  function handleSurfaceSelect(surface: string) {
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

<div
  bind:this={shellRootEl}
  class="flex h-screen w-screen flex-col text-surface-50 workshop-app-root"
  data-debug-label="app-root"
>
  <div class="flex min-h-0 flex-1" data-debug-label="app-row">
    <NavSidebar
      active={activeSurface}
      onSelect={handleSurfaceSelect}
      chatActivity={chat.backgroundActivity}
      workActivity={workspace.inMotionCount()}
      peersActivity={peersUnread}
      activeProfileLabel={userProfiles.activeDisplayName}
    />

    <div class="workshop-main relative flex min-w-0 flex-1 flex-col" data-debug-label="workshop-main">
      <EnvPendingProposalBanner />
      <div
        class="flex min-h-0 min-w-0 flex-1 overflow-hidden"
        data-debug-label="workshop-content-row"
      >
        <div
          class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
          data-debug-label="workshop-surface-column"
        >
        {#key layout.navigationEpoch}
          <EnvironmentRenderer surfaceId={activeSurface}>
            {#snippet builtin()}
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
          {:else if activeSurface === "peers"}
            <PeersPanel visible={true} />
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
            {/snippet}
          </EnvironmentRenderer>
        {/key}
        <div
          class="absolute inset-0 flex min-h-0 flex-col overflow-hidden"
          class:hidden={activeSurface !== "web"}
          aria-hidden={activeSurface !== "web"}
          data-debug-label="browser-surface-host"
        >
          <HumanBrowserPanel
            visible={activeSurface === "web"}
            workRailVisible={false}
          />
        </div>
        </div>

        {#if layout.activityCollapsed || desktopRails.showActivityStrip}
          <ActivityCollapsedStrip
            onExpand={() => layout.setActivityCollapsed(false)}
          />
        {:else}
          <div
            class="workshop-rail flex h-full min-w-0 shrink-0 overflow-hidden"
            data-debug-label="activity-rail"
          >
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
              browserUrl={browserContext.activeUrl}
              browserTitle={browserContext.scopeLabel}
              cardDetail={activeSurface === "work"
                ? null
                : workspace.selectedCardDetail}
              cardError={workspace.cardDetailError}
              noteDiffChip={vault.diffChipText}
              onOpenNote={handleOpenNote}
              onOpenWeb={() => navigateToSurface("web")}
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
        motionCards={workspace.railCards()}
        selectedMotionId={workspace.selectedCardId}
        onSelectMotion={handleCardSelect}
        onOpenRuntime={() => navigateToSurface("runtime")}
        onOpenCron={() => navigateToSurface("automations")}
        onOpenSpotlight={onOpenSpotlight}
      />
    </div>
  </div>

  <ShellLayoutDebug rootEl={shellRootEl} />
</div>
