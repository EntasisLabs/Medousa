<script lang="ts">
  import type { SidebarNode } from "../types";

  interface Props {
    tree: SidebarNode[];
    sizeLabel: string;
    warnings: string[];
  }

  let { tree, sizeLabel, warnings }: Props = $props();
</script>

<aside class="sidebar">
  <h2>Installation details</h2>
  {#if tree.length === 0}
    <p class="muted">Select workloads or components to see what will be installed.</p>
  {:else}
    <ul class="tree">
      {#each tree as node (node.id)}
        <li>
          <div class="tree-node">{node.label}</div>
          {#if node.children.length > 0}
            <ul>
              {#each node.children as child (child.id)}
                <li class="tree-child">
                  <span class="tree-marker">{child.included ? "✓" : "○"}</span>
                  {child.label}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#each warnings as warning}
    <p class="warning">{warning}</p>
  {/each}

  <div class="sidebar-footer">
    <div class="muted">Total space required</div>
    <div class="size-label">{sizeLabel}</div>
  </div>
</aside>
