<script lang="ts">
  import { onMount, untrack } from "svelte";
  import ShellPane from "$lib/components/shell/ShellPane.svelte";
  import ShellSplitNode from "$lib/components/shell/ShellSplitNode.svelte";
  import ShellPaneCheatSheet from "$lib/components/shell/ShellPaneCheatSheet.svelte";
  import { applyContentZoomCss } from "$lib/config/contentZoom";
  import { chat } from "$lib/stores/chat.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { attachShellPaneHotkeys } from "$lib/utils/shellPaneHotkeys";
  import { attachMouseShakeToolbar } from "$lib/utils/mouseShake";
  import type { DaemonHealth } from "$lib/daemon";

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

  let cheatSheetOpen = $state(false);

  /**
   * Single native webview host:
   * 1) focused group if its active tab is web
   * 2) else first group with an active web tab
   * Never assign ownership to a non-web focused pane (that stranded split browser panes).
   */
  const webOwnerGroupId = $derived.by(() => {
    const activeTabOf = (groupId: string) => {
      const group = shellTabs.groups.find((entry) => entry.id === groupId);
      if (!group) return null;
      return shellTabs.tabs.find((entry) => entry.id === group.activeTabId) ?? null;
    };

    const focused = activeTabOf(shellTabs.activeGroupId);
    if (focused?.kind === "web") return shellTabs.activeGroupId;

    for (const group of shellTabs.groups) {
      const tab = activeTabOf(group.id);
      if (tab?.kind === "web") return group.id;
    }
    return null;
  });

  $effect(() => {
    void chat.sessions;
    void lmeWorkspace.tabs;
    void humanBrowser.tabs;
    // Do not subscribe to shellTabs writes inside sync (avoids effect storms).
    untrack(() => shellTabs.syncTitlesFromStores());
  });

  $effect(() => {
    void lmeWorkspace.tabs;
    void lmeWorkspace.activeTabId;
    untrack(() => shellTabs.syncFromLmeWorkspace());
  });

  $effect(() => {
    void humanBrowser.tabs;
    void humanBrowser.activeTab?.id;
    untrack(() => shellTabs.syncFromHumanBrowser());
  });

  onMount(() => {
    shellTabs.bootstrap();
    chat.bootstrapMultiLive(shellTabs.chatSessionIdsForLiveRestore());
    applyContentZoomCss();
    const detachHotkeys = attachShellPaneHotkeys({
      onCheatSheet: () => {
        cheatSheetOpen = true;
      },
    });
    const detachShake = attachMouseShakeToolbar();
    return () => {
      detachHotkeys();
      detachShake();
    };
  });

  $effect(() => {
    void shellTabs.cheatSheetOpenRequest;
    if (shellTabs.cheatSheetOpenRequest > 0) {
      cheatSheetOpen = true;
    }
  });
</script>

<div
  class="shell-tab-host relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="shell-tab-host"
>
  {#key shellTabs.activeDesktopId}
    {#if shellTabs.zoomedGroupId}
      <ShellPane
        groupId={shellTabs.zoomedGroupId}
        {health}
        {onOpenChat}
        {onOpenWork}
        {onOpenContext}
        {onOpenConnection}
        {onOpenNote}
        {onSelectCard}
        {onDaemonHealth}
        ownsWebHost={webOwnerGroupId === shellTabs.zoomedGroupId}
      />
    {:else}
      <ShellSplitNode
        node={shellTabs.splitRoot}
        {health}
        {onOpenChat}
        {onOpenWork}
        {onOpenContext}
        {onOpenConnection}
        {onOpenNote}
        {onSelectCard}
        {onDaemonHealth}
        {webOwnerGroupId}
      />
    {/if}
  {/key}

  {#if cheatSheetOpen}
    <ShellPaneCheatSheet onClose={() => (cheatSheetOpen = false)} />
  {/if}
</div>
