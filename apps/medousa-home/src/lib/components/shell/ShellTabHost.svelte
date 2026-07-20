<script lang="ts">
  import { onMount } from "svelte";
  import ShellPane from "$lib/components/shell/ShellPane.svelte";
  import ShellSplitNode from "$lib/components/shell/ShellSplitNode.svelte";
  import ShellPaneCheatSheet from "$lib/components/shell/ShellPaneCheatSheet.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { attachShellPaneHotkeys } from "$lib/utils/shellPaneHotkeys";
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

  /** First group (in leaf order) whose active tab needs LME / web owns the shared host. */
  const lmeOwnerGroupId = $derived.by(() => {
    for (const group of shellTabs.groups) {
      const tab = shellTabs.tabs.find((entry) => entry.id === group.activeTabId);
      if (
        tab?.kind === "lme" ||
        (tab?.kind === "surface" && tab.surfaceId === "library")
      ) {
        return group.id;
      }
    }
    return shellTabs.activeGroupId;
  });

  const webOwnerGroupId = $derived.by(() => {
    for (const group of shellTabs.groups) {
      const tab = shellTabs.tabs.find((entry) => entry.id === group.activeTabId);
      if (tab?.kind === "web") return group.id;
    }
    return shellTabs.activeGroupId;
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
    return attachShellPaneHotkeys({
      onCheatSheet: () => {
        cheatSheetOpen = true;
      },
    });
  });
</script>

<div
  class="shell-tab-host relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="shell-tab-host"
>
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
      ownsLmeHost={lmeOwnerGroupId === shellTabs.zoomedGroupId}
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
      {lmeOwnerGroupId}
      {webOwnerGroupId}
    />
  {/if}

  {#if cheatSheetOpen}
    <ShellPaneCheatSheet onClose={() => (cheatSheetOpen = false)} />
  {/if}
</div>
