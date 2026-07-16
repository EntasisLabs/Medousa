<script lang="ts">
  /**
   * `tree` molecule — indented file/folder tree.
   * Paste-first from ```tree markdown.
   */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { renderInlineMarkdown } from "$lib/markdown";

  interface TreeNode {
    id: string;
    name: string;
    kind: "file" | "folder";
    children?: TreeNode[];
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(
    typeof node.props.subtitle === "string" ? node.props.subtitle : "",
  );

  function normalizeNodes(raw: unknown): TreeNode[] {
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const name = typeof row.name === "string" ? row.name.trim() : "";
        if (!name) return null;
        const kind = row.kind === "folder" ? "folder" : "file";
        const id = typeof row.id === "string" && row.id ? row.id : `tree-${i}`;
        const out: TreeNode = { id, name, kind };
        const kids = normalizeNodes(row.children);
        if (kids.length) {
          out.kind = "folder";
          out.children = kids;
        }
        return out;
      })
      .filter((n): n is TreeNode => n !== null);
  }

  const nodes = $derived(normalizeNodes(node.props.nodes));

  function selectNode(n: TreeNode) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { nodeId: n.id, name: n.name, kind: n.kind }),
    );
  }
</script>

{#snippet treeBranch(list: TreeNode[], depth: number)}
  <ul class="liquid-tree-list" role="group" style="--depth: {depth}">
    {#each list as child, i (child.id)}
      <li class="liquid-tree-node" style="--stagger: {depth + i * 0.15}">
        <button
          type="button"
          class="liquid-tree-row"
          class:liquid-tree-folder={child.kind === "folder"}
          onclick={() => selectNode(child)}
        >
          <span class="liquid-tree-icon" aria-hidden="true">
            {child.kind === "folder" ? "▸" : "·"}
          </span>
          <span class="liquid-tree-name">
            {child.name}{#if child.kind === "folder"}/{/if}
          </span>
        </button>
        {#if child.children?.length}
          {@render treeBranch(child.children, depth + 1)}
        {/if}
      </li>
    {/each}
  </ul>
{/snippet}

{#if nodes.length >= 1}
  <div class="liquid-tree" role="tree" aria-label={title || "Tree"}>
    {#if title || subtitle}
      <header class="liquid-tree-header">
        {#if title}
          <h3 class="liquid-tree-title">{@html renderInlineMarkdown(title)}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-tree-subtitle">{@html renderInlineMarkdown(subtitle)}</p>
        {/if}
      </header>
    {/if}
    {@render treeBranch(nodes, 0)}
  </div>
{/if}

<style>
  .liquid-tree {
    margin: 0;
    padding: 0.75rem 0.85rem 0.9rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    min-width: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .liquid-tree-header {
    margin-bottom: 0.55rem;
    font-family: inherit;
  }

  .liquid-tree-title {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 650;
    font-family: system-ui, sans-serif;
    color: rgb(var(--color-surface-50));
  }

  .liquid-tree-subtitle {
    margin: 0.3rem 0 0;
    font-size: 0.75rem;
    font-family: system-ui, sans-serif;
    color: rgb(var(--color-surface-400));
  }

  .liquid-tree-list {
    list-style: none;
    margin: 0;
    padding: 0 0 0 0.85rem;
  }

  .liquid-tree > .liquid-tree-list {
    padding-left: 0;
  }

  .liquid-tree-node {
    margin: 0;
  }

  .liquid-tree-row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    width: 100%;
    margin: 0;
    padding: 0.18rem 0.25rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    color: rgb(var(--color-surface-200));
    font: inherit;
    font-size: 0.78rem;
    text-align: left;
    cursor: pointer;
  }

  .liquid-tree-row:hover {
    background: color-mix(in srgb, var(--color-surface-700) 40%, transparent);
  }

  .liquid-tree-folder {
    color: rgb(var(--color-surface-50));
    font-weight: 600;
  }

  .liquid-tree-icon {
    flex: 0 0 auto;
    width: 0.85rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.7rem;
  }

  .liquid-tree-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
