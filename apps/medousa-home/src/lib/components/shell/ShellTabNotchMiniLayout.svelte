<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
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
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="shell-tab-notch-pane"
    class:shell-tab-notch-pane--active={active}
    class:shell-tab-notch-pane--drop={shellTabs.tabDropTargetGroupId === node.id}
    class:shell-tab-notch-pane--vacant={tabs.length === 0}
    data-group-id={node.id}
    role="group"
    aria-label={active ? "Active pane" : tabs.length ? "Pane" : "Empty pane"}
    onclick={(event) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest(".shell-tab-chip, .shell-tab-strip-nav, button")) return;
      shellTabs.focusGroup(node.id);
    }}
  >
    <div class="shell-tab-notch-pane-chrome">
      {#if tabs.length > 0}
        <ShellTabStrip groupId={node.id} variant="titlebar" {onTabSettled} />
      {:else}
        <span class="shell-tab-notch-pane-empty-label">Drop a tab here</span>
      {/if}
    </div>
    <div class="shell-tab-notch-pane-surface" aria-hidden="true"></div>
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
