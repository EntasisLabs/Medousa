<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import ShellTabNotchPaneRail from "$lib/components/shell/ShellTabNotchPaneRail.svelte";
  import { shellContextMenu } from "$lib/stores/shellContextMenu.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { MAX_SHELL_PANES, type SplitNode } from "$lib/types/shellTabs";
  import { Columns2, Rows2, SquareX } from "@lucide/svelte";

  interface Props {
    node: SplitNode;
    onTabSettled?: (info: { tabId: string; didMove: boolean }) => void;
  }

  let { node, onTabSettled }: Props = $props();

  const activeGroupId = $derived(shellTabs.activeGroupId);
  const canSplit = $derived(shellTabs.paneCount < MAX_SHELL_PANES);
  const canMergePane = $derived(shellTabs.paneCount > 1);
</script>

{#if node.type === "group"}
  {@const tabs = shellTabs.tabsForGroup(node.id)}
  {@const active = node.id === activeGroupId}
  {@const paneIndex = shellTabs.groups.findIndex((group) => group.id === node.id) + 1}
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
    role="button"
    tabindex="0"
    aria-label={active ? "Active pane" : tabs.length ? "Pane" : "Empty pane"}
    onclick={(event) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest(".shell-tab-notch-rail-row, button")) return;
      shellTabs.focusGroup(node.id);
    }}
    onkeydown={(event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        shellTabs.focusGroup(node.id);
      }
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
    <div class="shell-tab-notch-pane-chrome">
      <span>Pane {paneIndex}</span>
      {#if active}
        <div class="shell-tab-notch-pane-actions" role="group" aria-label="Active pane actions">
        <button
          type="button"
          title="Split pane right"
          aria-label="Split pane right"
          disabled={!canSplit}
          onclick={(event) => {
            event.stopPropagation();
            shellTabs.splitActive("right");
          }}
        ><Columns2 size={13} strokeWidth={1.8} /></button>
        <button
          type="button"
          title="Split pane down"
          aria-label="Split pane down"
          disabled={!canSplit}
          onclick={(event) => {
            event.stopPropagation();
            shellTabs.splitActive("down");
          }}
        ><Rows2 size={13} strokeWidth={1.8} /></button>
        <button
          type="button"
          title="Close pane and merge tabs"
          aria-label="Close pane and merge tabs"
          disabled={!canMergePane}
          onclick={(event) => {
            event.stopPropagation();
            shellTabs.closeActiveGroup();
          }}
        ><SquareX size={13} strokeWidth={1.8} /></button>
        </div>
      {/if}
    </div>
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
