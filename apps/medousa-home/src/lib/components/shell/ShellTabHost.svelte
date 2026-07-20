<script lang="ts">
  import { onMount } from "svelte";
  import HumanBrowserPanel from "$lib/components/browser/HumanBrowserPanel.svelte";
  import CalendarPanel from "$lib/components/calendar/CalendarPanel.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
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
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { refreshDaemonHealth } from "$lib/workshopConnection";

  interface Props {
    health?: DaemonHealth | null;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onOpenContext: () => void;
    onOpenConnection: () => void;
    onOpenNote: (path: string) => void | Promise<void>;
    onSelectCard: (id: string) => void | Promise<void>;
    onDaemonHealth?: (health: DaemonHealth | null) => void;
  }

  let {
    health = null,
    onOpenChat,
    onOpenWork,
    onOpenContext,
    onOpenConnection,
    onOpenNote,
    onSelectCard,
    onDaemonHealth,
  }: Props = $props();

  const active = $derived(shellTabs.activeTab);

  const showChat = $derived(active?.kind === "chat");
  const showLme = $derived(
    active?.kind === "lme" ||
      (active?.kind === "surface" && active.surfaceId === "library"),
  );
  const showWeb = $derived(active?.kind === "web");
  const showSurface = $derived(active?.kind === "surface" ? active.surfaceId : null);

  /** Keep chat/web/lme hosts mounted once opened (plan keep-alive policy). */
  let chatMounted = $state(false);
  let lmeMounted = $state(false);
  let webMounted = $state(false);

  $effect(() => {
    if (showChat || shellTabs.orderedTabs.some((tab) => tab.kind === "chat")) {
      chatMounted = true;
    }
  });
  $effect(() => {
    if (
      showLme ||
      shellTabs.orderedTabs.some((tab) => tab.kind === "lme" || (tab.kind === "surface" && tab.surfaceId === "library"))
    ) {
      lmeMounted = true;
    }
  });
  $effect(() => {
    if (showWeb || shellTabs.orderedTabs.some((tab) => tab.kind === "web")) {
      webMounted = true;
    }
  });

  $effect(() => {
    void chat.sessions;
    void lmeWorkspace.tabs;
    void humanBrowser.tabs;
    shellTabs.syncTitlesFromStores();
  });

  $effect(() => {
    void lmeWorkspace.tabs;
    void lmeWorkspace.activeTabId;
    shellTabs.syncFromLmeWorkspace();
  });

  $effect(() => {
    void humanBrowser.tabs;
    void humanBrowser.activeTab?.id;
    shellTabs.syncFromHumanBrowser();
  });

  onMount(() => {
    shellTabs.bootstrap();
  });

  const surfaceForCustom = $derived(
    showSurface && showSurface !== "library" ? showSurface : layoutDesktopHint(),
  );

  function layoutDesktopHint(): string {
    if (showChat) return "chat";
    if (showLme) return "library";
    if (showWeb) return "web";
    return showSurface ?? "chat";
  }
</script>

<div
  class="shell-tab-host flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="shell-tab-host"
>
  <ShellTabStrip />

  <div class="relative min-h-0 min-w-0 flex-1 overflow-hidden">
    {#if chatMounted}
      <div
        class="absolute inset-0 flex min-h-0 flex-col overflow-hidden"
        class:hidden={!showChat}
        aria-hidden={!showChat}
        data-debug-label="shell-tab-body-chat"
      >
        <ChatPanel
          visible={showChat}
          onOpenContext={onOpenContext}
          onOpenConnection={onOpenConnection}
        />
      </div>
    {/if}

    {#if lmeMounted}
      <div
        class="absolute inset-0 flex min-h-0 flex-col overflow-hidden"
        class:hidden={!showLme}
        aria-hidden={!showLme}
        data-debug-label="shell-tab-body-lme"
      >
        <LmePanel
          visible={showLme}
          {onOpenChat}
          {onOpenWork}
          {onSelectCard}
        />
      </div>
    {/if}

    {#if webMounted}
      <div
        class="absolute inset-0 flex min-h-0 flex-col overflow-hidden"
        class:hidden={!showWeb}
        aria-hidden={!showWeb}
        data-debug-label="shell-tab-body-web"
      >
        <HumanBrowserPanel visible={showWeb} workRailVisible={false} shellTabChrome={true} />
      </div>
    {/if}

    {#if showSurface && showSurface !== "library"}
      <div
        class="absolute inset-0 flex min-h-0 flex-col overflow-hidden"
        data-debug-label="shell-tab-body-surface"
      >
        <EnvironmentRenderer surfaceId={surfaceForCustom}>
          {#snippet builtin()}
            {#if showSurface === "calendar"}
              <CalendarPanel visible={true} />
            {:else if showSurface === "context"}
              <ContextPanel
                visible={true}
                onOpenChat={async (sessionId) => {
                  shellTabs.openChat(sessionId, { activate: true });
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
      </div>
    {/if}

    {#if !active}
      <div
        class="flex h-full items-center justify-center p-8 text-sm text-surface-500"
        data-debug-label="shell-tab-empty"
      >
        Open something from the rail.
      </div>
    {/if}
  </div>
</div>
