<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import ShellTabNotchPaneRail from "$lib/components/shell/ShellTabNotchPaneRail.svelte";
  import { shellContextMenu } from "$lib/stores/shellContextMenu.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import type { SplitNode } from "$lib/types/shellTabs";

  interface Props {
    node: SplitNode;
    onTabSettled?: (info: { tabId: string; didMove: boolean }) => void;
  }

  let { node, onTabSettled }: Props = $props();

  const activeGroupId = $derived(shellTabs.activeGroupId);
</script>

{#if node.type === "group"}
  {@const tabs = shellTabs.tabsForGroup(node.id)}
  {@const active = node.id === activeGroupId}
  {@const split =
    shellTabs.tabDropSplitEdge?.groupId === node.id
      ? shellTabs.tabDropSplitEdge.edge
      : null}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="shell-tab-notch-pane"
    class:shell-tab-notch-pane--active={active}
    class:shell-tab-notch-pane--drop={shellTabs.tabDropTargetGroupId === node.id}
    class:shell-tab-notch-pane--vacant={tabs.length === 0}
    class:shell-tab-notch-pane--split-left={split === "left"}
    class:shell-tab-notch-pane--split-right={split === "right"}
    class:shell-tab-notch-pane--split-top={split === "top"}
    class:shell-tab-notch-pane--split-bottom={split === "bottom"}
    data-group-id={node.id}
    role="group"
    aria-label={active ? "Active pane" : tabs.length ? "Pane" : "Empty pane"}
    onclick={(event) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest(".shell-tab-notch-rail-row, button")) return;
      shellTabs.focusGroup(node.id);
    }}
    oncontextmenu={(event) => {
      const hit = event.target as HTMLElement | null;
      if (hit?.closest(".shell-tab-notch-rail-row, button")) return;
      event.preventDefault();
      event.stopPropagation();
      shellTabs.focusGroup(node.id);
      shellContextMenu.showPane(event.clientX, event.clientY, node.id);
    }}
  >
    <ShellTabNotchPaneRail groupId={node.id} {onTabSettled} />
  </div>
{:else}
  {@const sideBySide = node.direction === "column"}
  <div
    class="shell-tab-notch-branch"
    class:shell-tab-notch-branch--row={sideBySide}
    class:shell-tab-notch-branch--col={!sideBySide}
  >
    <div class="shell-tab-notch-branch-child" style={`flex: ${node.ratio} 1 0%;`}>
      <ShellTabNotchMiniLayout node={node.a} {onTabSettled} />
    </div>
    <div class="shell-tab-notch-branch-sash" aria-hidden="true"></div>
    <div class="shell-tab-notch-branch-child" style={`flex: ${1 - node.ratio} 1 0%;`}>
      <ShellTabNotchMiniLayout node={node.b} {onTabSettled} />
    </div>
  </div>
{/if}
