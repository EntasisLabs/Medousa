<script lang="ts">
  import { Check, Circle } from "@lucide/svelte";
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
  {#if tree.length === 0}
    <p class="empty">Select workloads or components to see what will be installed.</p>
  {:else}
    <ul class="tree">
      {#each tree as node (node.id)}
        <li>
          <div class="tree-node">{node.label}</div>
          {#if node.children.length > 0}
            <ul>
              {#each node.children as child (child.id)}
                <li class="tree-child" class:optional={child.optional && !child.included}>
                  <span class="tree-marker" aria-hidden="true">
                    {#if child.included}
                      <Check size={14} strokeWidth={2.25} />
                    {:else}
                      <Circle size={14} strokeWidth={1.75} />
                    {/if}
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
</aside>

<style>
  .sidebar {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 1rem;
    position: sticky;
    top: 0;
  }

  .sidebar h2 {
    font-size: var(--installer-caption-size);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--installer-muted);
    margin: 0 0 0.75rem;
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

  .tree ul {
    list-style: none;
    margin: 0.35rem 0 0.5rem 0;
    padding: 0;
  }

  .tree-node {
    font-weight: 600;
    margin-bottom: 0.25rem;
  }

  .tree-child {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--installer-text-secondary);
    margin: 0.25rem 0;
  }

  .tree-child.optional {
    color: var(--installer-muted);
  }

  .tree-marker {
    color: var(--installer-accent);
    display: inline-flex;
    flex-shrink: 0;
  }

  .tree-child.optional .tree-marker {
    color: var(--installer-faint);
  }

  .warning {
    color: var(--installer-warning);
    font-size: var(--installer-caption-size);
    line-height: 1.45;
    margin: 0.75rem 0 0;
  }
</style>
