<script lang="ts">
  import HumanBrowserPanel from "$lib/components/browser/HumanBrowserPanel.svelte";
  import CalendarPanel from "$lib/components/calendar/CalendarPanel.svelte";
  import ChatSessionView from "$lib/components/chat/ChatSessionView.svelte";
  import ChatPaneIdle from "$lib/components/shell/ChatPaneIdle.svelte";
  import { chatStreamPool } from "$lib/stores/chatStreamPool.svelte";
  import ContextPanel from "$lib/components/context/ContextPanel.svelte";
  import EnvironmentRenderer from "$lib/components/environment/EnvironmentRenderer.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import LmePanel from "$lib/components/lme/LmePanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import PeersPanel from "$lib/components/peers/PeersPanel.svelte";
  import ProfilesPanel from "$lib/components/profiles/ProfilesPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import WorkPanel from "$lib/components/work/WorkPanel.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { refreshDaemonHealth } from "$lib/workshopConnection";

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

  /** Tabs only when the pointer is near the top of this pane (or drag/force). */
  const showTabs = $derived(
    tabs.length > 0 && (nearTop || overStrip || forceTabs),
  );
  const dropTarget = $derived(shellTabs.tabDropTargetGroupId === groupId);

  /** Live pool slot — not merely focused (multi-live transcripts). */
  const showLiveChat = $derived(
    activeTab?.kind === "chat" && chatStreamPool.isLive(activeTab.sessionId),
  );

  const showLme = $derived(
    activeTab?.kind === "lme" ||
      (activeTab?.kind === "surface" && activeTab.surfaceId === "library"),
  );
  const showWeb = $derived(ownsWebHost && activeTab?.kind === "web");
  const showSurface = $derived(
    activeTab?.kind === "surface" && activeTab.surfaceId !== "library"
      ? activeTab.surfaceId
      : null,
  );

  function focusPane() {
    if (!focused) shellTabs.focusGroup(groupId);
  }

  function handlePanePointerMove(event: PointerEvent) {
    const target = event.currentTarget as HTMLElement;
    const y = event.clientY - target.getBoundingClientRect().top;
    // Slightly taller zone while open so moving onto the strip feels natural.
    nearTop = y <= (showTabs ? 40 : 22);
  }

  function handlePanePointerLeave() {
    nearTop = false;
  }
</script>

<section
  class="shell-pane relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden
    {focused ? 'shell-pane-focused' : 'shell-pane-idle'}
    {dropTarget ? 'shell-pane-drop-target' : ''}"
  data-debug-label="shell-pane"
  data-group-id={groupId}
  role="group"
  aria-label="Editor pane"
  onpointermove={handlePanePointerMove}
  onpointerleave={handlePanePointerLeave}
  onpointerdown={focusPane}
>
  {#if showTabs}
    <div
      class="shell-pane-tabs pointer-events-auto absolute inset-x-0 top-0 z-30"
      onpointerenter={() => {
        overStrip = true;
      }}
      onpointerleave={() => {
        overStrip = false;
      }}
    >
      <ShellTabStrip {groupId} />
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
    {:else if showLme}
      <LmePanel
        visible={true}
        interactive={focused}
        lmeTabId={activeTab.kind === "lme" ? activeTab.lmeTabId : null}
        {onOpenChat}
        {onOpenWork}
        {onSelectCard}
      />
    {:else if showWeb}
      <HumanBrowserPanel visible={true} workRailVisible={false} shellTabChrome={true} />
    {:else if showSurface}
      <EnvironmentRenderer surfaceId={showSurface}>
        {#snippet builtin()}
          {#if showSurface === "calendar"}
            <CalendarPanel visible={true} />
          {:else if showSurface === "context"}
            <ContextPanel
              visible={true}
              onOpenChat={async (sessionId) => {
                shellTabs.openChat(sessionId, { activate: true, groupId });
                await chat.switchSession(sessionId);
              }}
            />
          {:else if showSurface === "profiles"}
            <ProfilesPanel visible={true} onOpenChat={onOpenChat} />
          {:else if showSurface === "peers"}
            <PeersPanel visible={true} />
          {:else if showSurface === "messaging"}
            <MessagingPanel visible={true} {health} />
          {:else if showSurface === "work"}
            <WorkPanel
              visible={true}
              {onOpenNote}
              {onOpenChat}
              {onSelectCard}
            />
          {:else if showSurface === "runtime"}
            <RuntimePanel visible={true} inMotionCount={workspace.inMotionCount()} />
          {:else if showSurface === "settings"}
            <SettingsPanel
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
    {:else if activeTab?.kind === "web"}
      <div class="flex h-full items-center justify-center px-6 text-center">
        <p class="workshop-faint text-xs leading-relaxed">
          Browser is open in another pane — focus that pane or open it here.
        </p>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center p-8 text-sm text-surface-500">
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
  .shell-pane-tabs {
    animation: shell-tabs-in 120ms ease-out;
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
