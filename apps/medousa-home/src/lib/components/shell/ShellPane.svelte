<script lang="ts">
  import HumanBrowserPanel from "$lib/components/browser/HumanBrowserPanel.svelte";
  import CalendarPanel from "$lib/components/calendar/CalendarPanel.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import ChatPaneIdle from "$lib/components/shell/ChatPaneIdle.svelte";
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
    /** Shared hosts: only the owning pane mounts LME/web. */
    ownsLmeHost: boolean;
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
    ownsLmeHost,
    ownsWebHost,
  }: Props = $props();

  let hovering = $state(false);

  const focused = $derived(shellTabs.activeGroupId === groupId);
  const group = $derived(shellTabs.groups.find((entry) => entry.id === groupId));
  const tabs = $derived(shellTabs.tabsForGroup(groupId));
  const activeTab = $derived(
    tabs.find((tab) => tab.id === group?.activeTabId) ?? tabs[0] ?? null,
  );

  const showTabs = $derived(
    hovering || focused || shellTabs.shouldForceShowTabs(groupId),
  );

  /** Focused chat pane owns the live ChatPanel; others show cached idle. */
  const showLiveChat = $derived(activeTab?.kind === "chat" && focused);

  const showLme = $derived(
    ownsLmeHost &&
      (activeTab?.kind === "lme" ||
        (activeTab?.kind === "surface" && activeTab.surfaceId === "library")),
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
</script>

<section
  class="shell-pane relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden
    {focused ? 'shell-pane-focused' : 'shell-pane-idle'}"
  data-debug-label="shell-pane"
  data-group-id={groupId}
  role="group"
  aria-label="Editor pane"
  onpointerenter={() => {
    hovering = true;
  }}
  onpointerleave={() => {
    hovering = false;
  }}
  onpointerdown={focusPane}
>
  <div
    class="shell-pane-tabs shrink-0 overflow-hidden transition-[max-height,opacity] duration-150
      {showTabs ? 'max-h-12 opacity-100' : 'max-h-0 opacity-0'}"
  >
    <ShellTabStrip {groupId} />
  </div>

  <div class="relative min-h-0 min-w-0 flex-1 overflow-hidden">
    {#if activeTab?.kind === "chat"}
      {#if showLiveChat}
        <ChatPanel
          visible={true}
          onOpenContext={onOpenContext}
          onOpenConnection={onOpenConnection}
        />
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
    {:else if activeTab?.kind === "lme" || activeTab?.kind === "web"}
      <div class="flex h-full items-center justify-center px-6 text-center">
        <p class="workshop-faint text-xs leading-relaxed">
          {activeTab.kind === "lme" ? "Workspace" : "Browser"} is open in another pane —
          focus that pane or open it here.
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
  @media (prefers-reduced-motion: reduce) {
    .shell-pane-tabs {
      transition: none;
    }
  }
</style>
