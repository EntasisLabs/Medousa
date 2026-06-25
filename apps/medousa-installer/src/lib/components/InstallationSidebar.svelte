<script lang="ts">
  import { Check } from "@lucide/svelte";
  import { humanizeWarning } from "../copy";
  import type { SidebarNode } from "../types";

  interface Props {
    tree: SidebarNode[];
    warnings: string[];
  }

  let { tree, warnings }: Props = $props();
</script>

<aside class="sidebar" aria-label="Installation summary">
  <h2>What you're getting</h2>

  <div class="sidebar-scroll scroll-pane">
    {#if tree.length === 0}
      <p class="empty">Select workloads or components to see what will be installed.</p>
    {:else}
      <ul class="tree">
        {#each tree as node (node.id)}
          <li class="tree-group">
            <div class="tree-node">{node.label}</div>
            {#if node.children.length > 0}
              <ul>
                {#each node.children as child (child.id)}
                  <li class="tree-child" class:addon={child.optional}>
                    <span class="tree-marker" aria-hidden="true">
                      <Check size={14} strokeWidth={2.25} />
                    </span>
                    <span>{child.label}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#each warnings as warning}
      <p class="warning">{humanizeWarning(warning)}</p>
    {/each}
  </div>
</aside>

<style>
  .sidebar {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 0.85rem 0 0.85rem 1rem;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .sidebar h2 {
    font-size: var(--installer-caption-size);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--installer-muted);
    margin: 0 0 0.65rem;
    padding-right: 1rem;
    flex-shrink: 0;
  }

  .sidebar-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 0.65rem;
  }

  .empty {
    color: var(--installer-muted);
    font-size: var(--installer-body-size);
    line-height: 1.5;
    margin: 0;
  }

  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: var(--installer-body-size);
  }

  .tree-group + .tree-group {
    margin-top: 0.75rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--installer-border);
  }

  .tree ul {
    list-style: none;
    margin: 0.25rem 0 0;
    padding: 0;
  }

  .tree-node {
    font-size: var(--installer-caption-size);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--installer-muted);
    margin-bottom: 0.15rem;
  }

  .tree-child {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--installer-text-secondary);
    margin: 0.2rem 0;
  }

  .tree-child.addon {
    color: var(--installer-muted);
  }

  .tree-marker {
    color: var(--installer-accent);
    display: inline-flex;
    flex-shrink: 0;
  }

  .warning {
    color: var(--installer-warning);
    font-size: var(--installer-caption-size);
    line-height: 1.45;
    margin: 0.75rem 0 0;
  }
</style>
