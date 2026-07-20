<script lang="ts">
  import { onMount } from "svelte";
  import MasterRailHost from "$lib/components/layout/MasterRailHost.svelte";
  import { connectWorkshop } from "$lib/workshopConnection";
  import ActivityCollapsedStrip from "$lib/components/layout/ActivityCollapsedStrip.svelte";
  import ActivityPanel from "$lib/components/layout/ActivityPanel.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import StatusBar from "$lib/components/layout/StatusBar.svelte";
  import ShellTabHost from "$lib/components/shell/ShellTabHost.svelte";
  import IdentityDrawer from "$lib/components/chat/IdentityDrawer.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { layoutDesktopRails } from "$lib/utils/desktopRails";
  import { peerUnreadCount } from "$lib/utils/lanShareApi";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { browserContext } from "$lib/stores/browserContext.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { isTauri } from "$lib/platform";
  import { updateTrayBlockedCount } from "$lib/window";
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
  const activeShell = $derived(shellTabs.activeTab);
  const showChatChrome = $derived(activeShell?.kind === "chat");

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

  const desktopChrome = $derived(environment.desktopShellChrome);
  const activityRailHidden = $derived(desktopChrome.activityRail === "hidden");
  const desktopRails = $derived(
    layoutDesktopRails({
      viewportWidth: layout.viewportWidth,
      activityCollapsed: layout.activityCollapsed || activityRailHidden,
      activityWidth: layout.activityWidth,
      workInspectorOpen: false,
      workInspectorWidth: layout.workInspectorWidth,
    }),
  );

  function navigateToSurface(surface: string) {
    // Automations + Capabilities fold into the LME workspace (library surface).
    if (surface === "automations") {
      const mode = lmeWorkspace.explorerMode;
      if (
        mode !== "scripts" &&
        mode !== "flows" &&
        mode !== "schedules" &&
        mode !== "history" &&
        mode !== "agents"
      ) {
        lmeWorkspace.setExplorerMode("scripts");
      }
      shellTabs.openSurface("library", { activate: true });
      return;
    }
    if (surface === "workshop") {
      lmeWorkspace.setExplorerMode("agents");
      shellTabs.openSurface("library", { activate: true });
      return;
    }
    if (surface === "chat") {
      void chat.refreshSessions();
      void chat.ensureSessionHydrated();
      const sessionId = chat.sessionId?.trim();
      if (sessionId) {
        shellTabs.openChat(sessionId, { activate: true });
      } else {
        shellTabs.openSurface("chat", { activate: true });
      }
      return;
    }
    if (surface === "work") {
      void workspace.prefetchCardDetails();
    }
    shellTabs.openDestination(surface);
  }

  function goToSurface(surface: string) {
    navigateToSurface(surface);
  }

  function handleSurfaceSelect(surface: string) {
    navigateToSurface(surface);
  }

  async function handleOpenNote(path: string) {
    await lmeWorkspace.openNote(path);
  }

  async function handleCardSelect(id: string) {
    shellTabs.openSurface("work", { activate: true });
    await workspace.selectCard(id);
  }
</script>

<div
  bind:this={shellRootEl}
  class="flex h-screen w-screen flex-col text-surface-50 workshop-app-root"
  data-debug-label="app-root"
>
  <div class="flex min-h-0 flex-1" data-debug-label="app-row">
    <MasterRailHost
      active={activeSurface}
      onSelect={handleSurfaceSelect}
      onOpenChat={() => goToSurface("chat")}
      health={daemonHealth}
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
          <ShellTabHost
            health={daemonHealth}
            onOpenChat={() => goToSurface("chat")}
            onOpenWork={() => goToSurface("work")}
            onOpenContext={() => {
              layout.setIdentityDrawerOpen(false);
              goToSurface("context");
            }}
            onOpenConnection={() => {
              settingsNav.openSection("models");
              goToSurface("settings");
            }}
            onOpenNote={handleOpenNote}
            onSelectCard={handleCardSelect}
            onDaemonHealth={(health) => {
              daemonHealth = health;
            }}
          />
        </div>

        {#if !activityRailHidden}
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
        {/if}
      </div>

      {#if showChatChrome}
        {#if layout.sessionDrawerOpen && !layout.shellSidebarExpanded}
          <SessionSidebar
            open={true}
            onClose={() => layout.setSessionDrawerOpen(false)}
          />
        {/if}
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
        minimal={showChatChrome}
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
