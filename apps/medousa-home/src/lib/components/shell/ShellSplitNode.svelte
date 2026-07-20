<script lang="ts">
  import EditorSplitSash from "$lib/components/shell/EditorSplitSash.svelte";
  import ShellPane from "$lib/components/shell/ShellPane.svelte";
  import ShellSplitNode from "$lib/components/shell/ShellSplitNode.svelte";
  import type { SplitNode } from "$lib/types/shellTabs";
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    node: SplitNode;
    health?: DaemonHealth | null;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onOpenContext: () => void;
    onOpenConnection: () => void;
    onOpenNote: (path: string) => void | Promise<void>;
    onSelectCard: (id: string) => void | Promise<void>;
    onDaemonHealth?: (health: DaemonHealth | null) => void;
    lmeOwnerGroupId: string | null;
    webOwnerGroupId: string | null;
  }

  let {
    node,
    health = null,
    onOpenChat,
    onOpenWork,
    onOpenContext,
    onOpenConnection,
    onOpenNote,
    onSelectCard,
    onDaemonHealth,
    lmeOwnerGroupId,
    webOwnerGroupId,
  }: Props = $props();
</script>

{#if node.type === "group"}
  <ShellPane
    groupId={node.id}
    {health}
    {onOpenChat}
    {onOpenWork}
    {onOpenContext}
    {onOpenConnection}
    {onOpenNote}
    {onSelectCard}
    {onDaemonHealth}
    ownsLmeHost={lmeOwnerGroupId === node.id}
    ownsWebHost={webOwnerGroupId === node.id}
  />
{:else}
  {@const vertical = node.direction === "column"}
  <div
    class="flex h-full min-h-0 min-w-0 flex-1 overflow-hidden {vertical
      ? 'flex-row'
      : 'flex-col'}"
    data-debug-label="shell-split-branch"
  >
    <div
      class="flex min-h-0 min-w-0 overflow-hidden"
      style={`flex: ${node.ratio} 1 0%;`}
    >
      <ShellSplitNode
        node={node.a}
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
    </div>
    <EditorSplitSash
      branchId={node.id}
      direction={node.direction}
      ratio={node.ratio}
    />
    <div
      class="flex min-h-0 min-w-0 overflow-hidden"
      style={`flex: ${1 - node.ratio} 1 0%;`}
    >
      <ShellSplitNode
        node={node.b}
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
    </div>
  </div>
{/if}
