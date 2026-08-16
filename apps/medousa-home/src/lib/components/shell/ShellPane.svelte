<script lang="ts">
  import ChatSessionView from "$lib/components/chat/ChatSessionView.svelte";
  import LazyFeatureView from "$lib/components/layout/LazyFeatureView.svelte";
  import ShellChunkError from "$lib/components/layout/ShellChunkError.svelte";
  import ChatPaneIdle from "$lib/components/shell/ChatPaneIdle.svelte";
  import WebPaneIdle from "$lib/components/shell/WebPaneIdle.svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import { chatStreamPool } from "$lib/chat/chatStreamPool.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { usesUnifiedTitlebar } from "$lib/platform";
  import type { DaemonHealth } from "$lib/daemon";
  import { refreshDaemonHealth } from "$lib/workshopConnection";
  import {
    loadCalendarPanel,
    loadEnvironmentRenderer,
    loadHumanBrowserPanel,
    loadLmePanel,
    loadMapPanel,
    loadMessagingPanel,
    loadPeersPanel,
    loadProfilesPanel,
    loadRuntimePanel,
    loadSettingsPanel,
    loadTerminalPane,
    loadWorkPanel,
  } from "$lib/runtime/viewLoaders";

  interface Props {
    groupId: string;
    health?: DaemonHealth | null;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onOpenContext: () => void;
    onOpenConnection: () => void;
    onOpenNote: (path: string) => void | Promise<void>;
    onSelectCard: (id: string) => void | Promise<void>;
    onDaemonHealth?: (health: DaemonHealth | null) => void;
    /** Browser still single-hosts (webview); Workspace mounts per pane. */
    ownsWebHost: boolean;
  }

  let {
    groupId,
    health = null,
    onOpenChat,
    onOpenWork,
    onOpenContext,
    onOpenConnection,
    onOpenNote,
    onSelectCard,
    onDaemonHealth,
    ownsWebHost,
  }: Props = $props();

  let surfaceChunkEpoch = $state(0);

  /** Pointer in the top hot-zone (reveals tabs). */
  let nearTop = $state(false);
  /** Pointer over the revealed strip (keeps it open while using tabs). */
  let overStrip = $state(false);

  const focused = $derived(shellTabs.activeGroupId === groupId);
  const group = $derived(shellTabs.groups.find((entry) => entry.id === groupId));
  const tabs = $derived(shellTabs.tabsForGroup(groupId));
  const activeTab = $derived(
    tabs.find((tab) => tab.id === group?.activeTabId) ?? tabs[0] ?? null,
  );

  const forceTabs = $derived(
    shellTabs.shouldForceShowTabs(groupId) ||
      shellTabs.tabDropTargetGroupId === groupId,
  );

  /**
   * Desktop Tauri hosts tabs in AppTitlebar — no in-pane strip (avoids double rows,
   * including webview panes). Browser / mobile keep hover or in-flow tabs.
   */
  const unifiedTitlebar = $derived(usesUnifiedTitlebar());
  const webChrome = $derived(activeTab?.kind === "web");
  const showTabs = $derived(
    !unifiedTitlebar &&
      tabs.length > 0 &&
      (webChrome || nearTop || overStrip || forceTabs),
  );
  /** Room for flow/agent titlebar actions when hover tabs are still in-pane. */
  const tabStripOffsetRight = "11.5rem";
  const tabActionReservePx = 184;
  const dropTarget = $derived(shellTabs.tabDropTargetGroupId === groupId);
  const splitEdge = $derived(
    shellTabs.tabDropSplitEdge?.groupId === groupId
      ? shellTabs.tabDropSplitEdge.edge
      : null,
  );

  /** Live pool slot — not merely focused (multi-live transcripts). */
  const showLiveChat = $derived(
    activeTab?.kind === "chat" && chatStreamPool.isLive(activeTab.sessionId),
  );

  const showLme = $derived(
    activeTab?.kind === "lme" ||
      (activeTab?.kind === "surface" &&
        (activeTab.surfaceId === "library" || activeTab.surfaceId === "code")),
  );
  const showWeb = $derived(ownsWebHost && activeTab?.kind === "web");
  const showTerminal = $derived(activeTab?.kind === "terminal");
  const showSurface = $derived(
    activeTab?.kind === "surface" &&
      activeTab.surfaceId !== "library" &&
      activeTab.surfaceId !== "code"
      ? activeTab.surfaceId
      : null,
  );

  function focusPane() {
    if (!focused) shellTabs.focusGroup(groupId);
  }

  function handlePanePointerMove(event: PointerEvent) {
    if (unifiedTitlebar) return;
    const target = event.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const y = event.clientY - rect.top;
    const x = event.clientX - rect.left;
    // Don't reveal tabs while aiming at right-side titlebar actions.
    const inActionZone = x >= rect.width - tabActionReservePx;
    // Slightly taller zone while open so moving onto the strip feels natural.
    nearTop = !inActionZone && y <= (showTabs ? 40 : 22);
  }

  function handlePanePointerLeave() {
    nearTop = false;
  }
</script>

<section
  class="shell-pane relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden
    {focused ? 'shell-pane-focused' : 'shell-pane-idle'}
    {dropTarget ? 'shell-pane-drop-target' : ''}
    {splitEdge === 'left' ? 'shell-pane-split-left' : ''}
    {splitEdge === 'right' ? 'shell-pane-split-right' : ''}
    {splitEdge === 'top' ? 'shell-pane-split-top' : ''}
    {splitEdge === 'bottom' ? 'shell-pane-split-bottom' : ''}"
  data-debug-label="shell-pane"
  data-group-id={groupId}
  role="group"
  aria-label="Editor pane"
  onpointermove={unifiedTitlebar ? undefined : handlePanePointerMove}
  onpointerleave={unifiedTitlebar ? undefined : handlePanePointerLeave}
  onpointerdown={focusPane}
>
  {#if showTabs && webChrome}
    <!-- In-flow strip: native webview/chrome would otherwise hide hover tabs. -->
    <div class="shell-pane-tabs shell-pane-tabs-web shrink-0">
      <ShellTabStrip {groupId} />
    </div>
  {:else if showTabs}
    <!-- Overlay is pointer-events-none so view chrome stays clickable; only the
         bounded tab strip captures pointer. Right inset clears action clusters. -->
    <div
      class="shell-pane-tabs pointer-events-none absolute inset-x-0 top-0 z-30"
      style:padding-right={tabStripOffsetRight}
    >
      <div
        class="pointer-events-auto w-full min-w-0"
        role="presentation"
        onpointerenter={() => {
          overStrip = true;
        }}
        onpointerleave={() => {
          overStrip = false;
        }}
      >
        <ShellTabStrip {groupId} />
      </div>
    </div>
  {/if}

  <div class="relative min-h-0 min-w-0 flex-1 overflow-hidden">
    {#if activeTab?.kind === "chat"}
      {#if showLiveChat}
        {#key activeTab.sessionId}
          <ChatSessionView
            sessionId={activeTab.sessionId}
            interactive={focused}
            visible={true}
            {onOpenContext}
            {onOpenConnection}
          />
        {/key}
      {:else}
        <ChatPaneIdle
          {groupId}
          sessionId={activeTab.sessionId}
          title={activeTab.title}
        />
      {/if}
    {:else if activeTab?.kind === "terminal"}
      {#key activeTab.sessionId}
        <LazyFeatureView
          loader={loadTerminalPane}
          sessionId={activeTab.sessionId}
          workId={activeTab.workId}
          title={activeTab.title}
        />
      {/key}
    {:else if showLme}
      <LazyFeatureView
        loader={loadLmePanel}
        visible={true}
        interactive={focused}
        lmeTabId={activeTab.kind === "lme" ? activeTab.lmeTabId : null}
        useActiveTabWhenUnbound={activeTab.kind !== "surface" || activeTab.surfaceId !== "code"}
        emptyMessage={activeTab.kind === "surface" && activeTab.surfaceId === "code"
          ? "Choose a project from the side panel, or start a new one."
          : "Open something from the side panel."}
        {onOpenChat}
        {onOpenWork}
        {onSelectCard}
      />
    {:else if showWeb}
      <LazyFeatureView
        loader={loadHumanBrowserPanel}
        visible={true}
        workRailVisible={false}
        shellTabChrome={true}
      />
    {:else if showSurface}
      {#key surfaceChunkEpoch}
        {#await loadEnvironmentRenderer()}
          <div class="flex h-full items-center justify-center p-8 text-sm text-content-quiet">
            Loading…
          </div>
        {:then { default: EnvironmentRenderer }}
          <EnvironmentRenderer surfaceId={showSurface}>
            {#snippet builtin()}
              {#if showSurface === "calendar"}
                <LazyFeatureView loader={loadCalendarPanel} visible={true} />
              {:else if showSurface === "context" || showSurface === "map"}
                <LazyFeatureView loader={loadMapPanel} visible={true} />
              {:else if showSurface === "profiles"}
                <LazyFeatureView loader={loadProfilesPanel} visible={true} {onOpenChat} />
              {:else if showSurface === "peers"}
                <LazyFeatureView loader={loadPeersPanel} visible={true} />
              {:else if showSurface === "messaging"}
                <LazyFeatureView loader={loadMessagingPanel} visible={true} {health} />
              {:else if showSurface === "work"}
                <LazyFeatureView
                  loader={loadWorkPanel}
                  visible={true}
                  {onOpenNote}
                  {onOpenChat}
                  {onSelectCard}
                />
              {:else if showSurface === "runtime"}
                <LazyFeatureView
                  loader={loadRuntimePanel}
                  visible={true}
                  inMotionCount={workspace.inMotionCount()}
                />
              {:else if showSurface === "settings"}
                <LazyFeatureView
                  loader={loadSettingsPanel}
                  visible={true}
                  revision={workspace.revision}
                  {health}
                  onDaemonHealth={async () => {
                    const next = await refreshDaemonHealth();
                    onDaemonHealth?.(next);
                  }}
                />
              {/if}
            {/snippet}
          </EnvironmentRenderer>
        {:catch}
          <ShellChunkError
            onRetry={() => {
              surfaceChunkEpoch += 1;
            }}
          />
        {/await}
      {/key}
    {:else if activeTab?.kind === "web"}
      <WebPaneIdle {groupId} />
    {:else}
      <div class="flex h-full items-center justify-center p-8 text-sm text-content-quiet">
        Open something from the rail.
      </div>
    {/if}
  </div>
</section>

<style>
  .shell-pane-focused {
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--color-primary-400, #a78bfa) 55%, transparent);
  }
  .shell-pane-idle {
    opacity: 0.92;
  }
  .shell-pane-drop-target {
    box-shadow: inset 0 0 0 2px color-mix(in oklab, var(--color-primary-400, #a78bfa) 80%, transparent);
    background: color-mix(in oklab, var(--color-primary-500, #8b5cf6) 8%, transparent);
  }
  .shell-pane-split-left {
    background: linear-gradient(
      to right,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 0%,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 28%,
      transparent 28%
    );
    box-shadow: inset 3px 0 0 0 color-mix(in oklab, var(--color-primary-400, #a78bfa) 85%, transparent);
  }
  .shell-pane-split-right {
    background: linear-gradient(
      to left,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 0%,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 28%,
      transparent 28%
    );
    box-shadow: inset -3px 0 0 0 color-mix(in oklab, var(--color-primary-400, #a78bfa) 85%, transparent);
  }
  .shell-pane-split-top {
    background: linear-gradient(
      to bottom,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 0%,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 28%,
      transparent 28%
    );
    box-shadow: inset 0 3px 0 0 color-mix(in oklab, var(--color-primary-400, #a78bfa) 85%, transparent);
  }
  .shell-pane-split-bottom {
    background: linear-gradient(
      to top,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 0%,
      color-mix(in oklab, var(--color-primary-500, #8b5cf6) 22%, transparent) 28%,
      transparent 28%
    );
    box-shadow: inset 0 -3px 0 0 color-mix(in oklab, var(--color-primary-400, #a78bfa) 85%, transparent);
  }
  .shell-pane-tabs {
    animation: shell-tabs-in 120ms ease-out;
  }
  .shell-pane-tabs-web {
    border-bottom: 1px solid
      color-mix(in oklab, var(--color-surface-500, #78716c) 35%, transparent);
    background: color-mix(
      in oklab,
      var(--color-surface-900, #1c1917) 55%,
      transparent
    );
  }
  @keyframes shell-tabs-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  :global(body.shell-tab-dragging) {
    cursor: grabbing;
    user-select: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .shell-pane-tabs {
      animation: none;
    }
  }
</style>
