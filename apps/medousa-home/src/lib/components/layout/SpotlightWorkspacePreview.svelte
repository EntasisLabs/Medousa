<script lang="ts">
  import type {
    EditorGroup,
    ShellDesktopLayout,
    ShellTab,
    SplitNode,
  } from "$lib/types/shellTabs";

  interface Props {
    layout: ShellDesktopLayout;
    selectedTabId?: string | null;
  }

  let { layout, selectedTabId = null }: Props = $props();

  const tabById = $derived(new Map(layout.tabs.map((tab) => [tab.id, tab])));

  function groupFor(id: string): EditorGroup | undefined {
    return layout.groups.find((group) => group.id === id);
  }

  function tabsFor(group: EditorGroup | undefined): ShellTab[] {
    if (!group) return [];
    return group.tabIds
      .map((id) => tabById.get(id))
      .filter((tab): tab is ShellTab => Boolean(tab));
  }

  function kindLabel(tab: ShellTab): string {
    if (tab.kind === "lme") return "note";
    if (tab.kind === "surface") return tab.surfaceId;
    return tab.kind;
  }
</script>

{#snippet renderNode(node: SplitNode)}
  {#if node.type === "group"}
    {@const group = groupFor(node.id)}
    {@const tabs = tabsFor(group)}
    {@const active = tabs.find((tab) => tab.id === group?.activeTabId) ?? tabs[0]}
    <section
      class="spotlight-layout-pane"
      class:spotlight-layout-pane--selected={Boolean(
        selectedTabId && tabs.some((tab) => tab.id === selectedTabId),
      )}
    >
      <div class="spotlight-layout-tabs">
        {#each tabs.slice(0, 3) as tab (tab.id)}
          <span
            class="spotlight-layout-tab"
            class:spotlight-layout-tab--active={tab.id === (selectedTabId ?? group?.activeTabId)}
          >
            {tab.title}
          </span>
        {:else}
          <span class="spotlight-layout-tab spotlight-layout-tab--empty">Empty pane</span>
        {/each}
        {#if tabs.length > 3}
          <span class="spotlight-layout-tab-count">+{tabs.length - 3}</span>
        {/if}
      </div>
      <div class="spotlight-layout-pane-body">
        {#if active}
          <span class="spotlight-layout-kind">{kindLabel(active)}</span>
          <span class="spotlight-layout-title">{active.title}</span>
          <span class="spotlight-layout-line spotlight-layout-line--long"></span>
          <span class="spotlight-layout-line"></span>
          <span class="spotlight-layout-line spotlight-layout-line--short"></span>
        {:else}
          <span class="spotlight-layout-empty">Open something from the rail</span>
        {/if}
      </div>
    </section>
  {:else}
    <div
      class="spotlight-layout-branch"
      class:spotlight-layout-branch--row={node.direction === "row"}
      class:spotlight-layout-branch--column={node.direction === "column"}
    >
      <div style:flex={node.ratio}>{@render renderNode(node.a)}</div>
      <div class="spotlight-layout-divider"></div>
      <div style:flex={1 - node.ratio}>{@render renderNode(node.b)}</div>
    </div>
  {/if}
{/snippet}

<div class="spotlight-workspace-map" aria-label="Workspace layout preview">
  {@render renderNode(layout.splitRoot)}
</div>

<style>
  .spotlight-workspace-map {
    display: flex;
    min-height: 13rem;
    height: 100%;
    overflow: hidden;
    border: 1px solid rgb(var(--theme-border) / 0.34);
    border-radius: calc(var(--theme-control-radius) * 0.78);
    background: rgb(var(--theme-canvas) / 0.32);
  }

  .spotlight-layout-branch {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .spotlight-layout-branch > div:not(.spotlight-layout-divider) {
    display: flex;
    min-width: 0;
    min-height: 0;
  }

  .spotlight-layout-branch--row {
    flex-direction: row;
  }

  .spotlight-layout-branch--column {
    flex-direction: column;
  }

  .spotlight-layout-branch--row > .spotlight-layout-divider {
    width: 1px;
  }

  .spotlight-layout-branch--column > .spotlight-layout-divider {
    height: 1px;
  }

  .spotlight-layout-divider {
    flex: 0 0 auto;
    background: rgb(var(--theme-border) / 0.38);
  }

  .spotlight-layout-pane {
    display: flex;
    width: 100%;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    background: rgb(var(--theme-pane) / 0.52);
    box-shadow: inset 0 0 0 1px transparent;
  }

  .spotlight-layout-pane--selected {
    background: rgb(var(--theme-card-hover) / 0.32);
  }

  .spotlight-layout-tabs {
    display: flex;
    min-width: 0;
    height: 1.75rem;
    align-items: stretch;
    overflow: hidden;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.25);
  }

  .spotlight-layout-tab {
    display: flex;
    min-width: 0;
    max-width: 8.5rem;
    align-items: center;
    overflow: hidden;
    padding: 0 0.55rem;
    border-radius: 0.3rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spotlight-layout-tab--active {
    background: rgb(var(--theme-card-hover) / 0.78);
    color: rgb(var(--theme-text));
  }

  .spotlight-layout-tab--empty {
    font-style: italic;
    color: rgb(var(--theme-text-faint));
  }

  .spotlight-layout-tab-count {
    margin-left: auto;
    align-self: center;
    padding-right: 0.45rem;
    color: rgb(var(--theme-text-faint));
    font-size: 0.625rem;
  }

  .spotlight-layout-pane-body {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    align-items: flex-start;
    padding: 0.8rem;
  }

  .spotlight-layout-kind {
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.56rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .spotlight-layout-title {
    max-width: 100%;
    overflow: hidden;
    margin-top: 0.35rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spotlight-layout-line {
    width: 62%;
    height: 0.24rem;
    margin-top: 0.45rem;
    border-radius: 999px;
    background: rgb(var(--theme-text-faint) / 0.22);
  }

  .spotlight-layout-line--long {
    width: 86%;
    margin-top: 0.8rem;
  }

  .spotlight-layout-line--short {
    width: 42%;
  }

  .spotlight-layout-empty {
    margin: auto;
    color: rgb(var(--theme-text-faint));
    font-size: 0.625rem;
  }
</style>
