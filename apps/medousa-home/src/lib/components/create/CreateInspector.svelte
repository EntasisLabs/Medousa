<script lang="ts">
  import { tick, untrack } from "svelte";
  import { layerRows, arrangeSelection, scaleSelection, objectBounds } from "$lib/create/creativeSelection";
  import {
    Eye,
    EyeOff,
    Lock,
    Unlock,
    ArrowUp,
    ArrowDown,
    Type,
    Image,
    Frame,
    PenTool,
    Square,
    Layers,
    X,
    ChevronRight,
    ChevronDown,
  } from "@lucide/svelte";
  import type {
    CreativeNode,
    CreativeOperation,
  } from "$lib/types/generated/daemon_api";
  import {
    nodeName,
    editableNode,
    resizeNode,
  } from "$lib/create/creativeScene";
  let {
    nodes,
    selected,
    disabled,
    onselect,
    oncommit,
    onclose,
  }: {
    nodes: CreativeNode[];
    selected: string[];
    disabled: boolean;
    onselect: (ids: string[]) => void;
    oncommit: (label: string, operations: CreativeOperation[]) => void;
    onclose: () => void;
  } = $props();
  const node = $derived(selected.length === 1 ? nodes.find((n) => n.id === selected[0]) : undefined);
  let expandedGroups = $state<string[]>([]), collapsedFrames = $state<string[]>([]);
  let renaming = $state<string | null>(null), renameDraft = $state(""), renameError = $state("");
  let renameInput = $state<HTMLInputElement>(), asideElement = $state<HTMLElement>();
  const collapsed = (n: CreativeNode) => n.content.kind === "group" ? !expandedGroups.includes(n.id) : collapsedFrames.includes(n.id);
  const rows = $derived(layerRows(nodes, collapsed));
  const selectionKey = $derived(selected.join("|"));
  $effect(() => {
    void selectionKey;
    untrack(() => {
      for (const id of selected) {
        let parent = nodes.find(n => n.id === nodes.find(n => n.id === id)?.parent_id);
        while (parent) {
          if (parent.content.kind === "group" && !expandedGroups.includes(parent.id)) expandedGroups = [...expandedGroups, parent.id];
          if (parent.content.kind === "frame") collapsedFrames = collapsedFrames.filter(id => id !== parent!.id);
          parent = nodes.find(n => n.id === parent!.parent_id);
        }
      }
    });
  });
  function toggle(layer: CreativeNode) {
    if (layer.content.kind === "group") expandedGroups = expandedGroups.includes(layer.id) ? expandedGroups.filter(id => id !== layer.id) : [...expandedGroups, layer.id];
    else collapsedFrames = collapsedFrames.includes(layer.id) ? collapsedFrames.filter(id => id !== layer.id) : [...collapsedFrames, layer.id];
  }
  function selectLayer(layer: CreativeNode, event: MouseEvent | KeyboardEvent) {
    if (event.shiftKey && selected.length) {
      const a = rows.findIndex(row => row.node.id === selected[selected.length - 1]), b = rows.findIndex(row => row.node.id === layer.id);
      if (a >= 0) { onselect([...new Set([...selected, ...rows.slice(Math.min(a, b), Math.max(a, b) + 1).map(r => r.node.id)])]); return; }
    }
    onselect(event.metaKey || event.ctrlKey ? selected.includes(layer.id) ? selected.filter(id => id !== layer.id) : [...selected, layer.id] : [layer.id]);
  }
  async function beginRename(layer: CreativeNode) {
    if (!editableNode(layer, nodes) || disabled) return;
    renaming = layer.id; renameDraft = nodeName(layer); renameError = "";
    await tick(); renameInput?.focus(); renameInput?.select();
  }
  function finishRename() {
    const layer = nodes.find(n => n.id === renaming), name = renameDraft.trim();
    if (!layer) return;
    if (new TextEncoder().encode(name).length > 256) { renameError = "Use a shorter layer name."; return; }
    renaming = null;
    if (name && name !== nodeName(layer)) update({ ...layer, name, content: layer.content.kind === "frame" ? { ...layer.content, name } : layer.content }, "Rename layer");
  }
  function layerKey(layer: CreativeNode, event: KeyboardEvent) {
    if (event.key === "F2") { event.preventDefault(); void beginRename(layer); return; }
    if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
      event.preventDefault();
      const row = rows.find(r => r.node.id === layer.id)!;
      if (row.children && (event.key === "ArrowRight") === collapsed(layer)) toggle(layer);
      else if (event.key === "ArrowLeft" && layer.parent_id) {
        onselect([layer.parent_id]); asideElement?.querySelector<HTMLButtonElement>(`[data-layer-id="${layer.parent_id}"] .layer-name`)?.focus();
      }
    }
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const i = rows.findIndex(r => r.node.id === layer.id), next = rows[i + (event.key === "ArrowUp" ? -1 : 1)];
      if (next) { selectLayer(next.node, event); asideElement?.querySelector<HTMLButtonElement>(`[data-layer-id="${next.node.id}"] .layer-name`)?.focus(); }
    }
  }
  const editable = $derived(node && editableNode(node, nodes) && !disabled);
  const icon = (n: CreativeNode) =>
    n.content.kind === "frame"
      ? Frame
      : n.content.kind === "text"
        ? Type
        : n.content.kind === "image"
          ? Image
          : n.content.kind === "ink"
            ? PenTool
            : n.content.kind === "group"
              ? Layers
              : Square;
  function update(next: CreativeNode, label = "Edit selection") {
    oncommit(label, [{ op: "put_node", node: next }]);
  }
  function transform(key: keyof CreativeNode["transform"], event: Event) {
    if (!node) return;
    const value = Number((event.target as HTMLInputElement).value);
    if (Number.isFinite(value) && node.content.kind === "group" && (key === "width" || key === "height")) {
      const b = objectBounds(node, nodes), factor = Math.max(1, value) / Math.max(1, node.transform[key]);
      const scaled = scaleSelection([node.id], nodes, { x: b.x, y: b.y }, factor);
      const operations = scaled.filter((n, i) => n !== nodes[i]).map(node => ({ op: "put_node" as const, node }));
      if (operations.length <= 1000) oncommit("Resize group", operations);
      return;
    }
    if (Number.isFinite(value))
      update(
        key === "width"
          ? resizeNode(node, value, node.transform.height)
          : key === "height"
            ? resizeNode(node, node.transform.width, value)
            : { ...node, transform: { ...node.transform, [key]: value } },
      );
  }
  function text(event: Event) {
    if (node?.content.kind === "text")
      update(
        {
          ...node,
          content: {
            ...node.content,
            text: (event.target as HTMLTextAreaElement).value,
          },
        },
        "Edit text",
      );
  }
  function color(value: string) {
    if (!node) return;
    const c = node.content;
    if (c.kind === "text" || c.kind === "ink")
      update({ ...node, content: { ...c, color: value } });
    else if (c.kind === "shape")
      update({ ...node, content: { ...c, fill: value } });
    else if (c.kind === "frame")
      update({ ...node, content: { ...c, background: value } });
  }
  const fill = $derived(
    node?.content.kind === "text" || node?.content.kind === "ink"
      ? node.content.color
      : node?.content.kind === "shape"
        ? node.content.fill
        : node?.content.kind === "frame"
          ? node.content.background
          : null,
  );
  function reorder(direction: number) {
    if (!node) return;
    const operations = arrangeSelection([node.id], nodes, direction as -1 | 1);
    if (operations.length) oncommit("Reorder layer", operations);
  }
  function crop(key: "x" | "y" | "width" | "height", value: number) {
    if (node?.content.kind !== "image" || !Number.isFinite(value)) return;
    const c = {
      x: 0,
      y: 0,
      width: 1,
      height: 1,
      ...node.content.crop,
      [key]: Math.max(0, Math.min(1, value / 100)),
    };
    c.x = Math.min(0.99, c.x);
    c.y = Math.min(0.99, c.y);
    c.width = Math.max(0.01, Math.min(c.width, 1 - c.x));
    c.height = Math.max(0.01, Math.min(c.height, 1 - c.y));
    update({ ...node, content: { ...node.content, crop: c } }, "Crop image");
  }

</script>

<aside bind:this={asideElement} class="create-inspector" aria-label="Selection and layers">
  <header>
    <strong>{node ? nodeName(node) : selected.length > 1 ? `${selected.length} objects` : "Project"}</strong><button
      type="button"
      onclick={onclose}
      aria-label="Close inspector"><X size={15} /></button
    >
  </header>
  {#if node}
    <div class="create-properties">
      <div class="property-grid">
        {#each ["x", "y", "width", "height", "rotation"] as key}<label
            ><span
              >{key === "width"
                ? "W"
                : key === "height"
                  ? "H"
                  : key === "rotation"
                    ? "°"
                    : key.toUpperCase()}</span
            ><input
              aria-label={key}
              type="number"
              step="1"
              value={Math.round(
                node.transform[key as keyof CreativeNode["transform"]] * 100,
              ) / 100}
              disabled={!editable}
              onchange={(e) =>
                transform(key as keyof CreativeNode["transform"], e)}
            /></label
          >{/each}
        <label
          ><span>α</span><input
            aria-label="Opacity"
            type="number"
            min="0"
            max="1"
            step=".05"
            value={node.opacity}
            disabled={!editable}
            onchange={(e) =>
              node &&
              update({
                ...node,
                opacity: Math.max(
                  0,
                  Math.min(1, Number(e.currentTarget.value)),
                ),
              })}
          /></label
        >
      </div>
      {#if fill}<label class="property-color"
          >Fill<input
            type="color"
            aria-label="Selection color"
            value={/^#[0-9a-f]{6}$/i.test(fill) ? fill : "#393e33"}
            disabled={!editable}
            onchange={(e) => color(e.currentTarget.value)}
          /></label
        >{/if}
      {#if node.content.kind === "text"}
        <label class="property-stack"
          >Text<textarea
            aria-label="Text content"
            value={node.content.text}
            disabled={!editable}
            onchange={text}></textarea></label
        >
        <label class="property-color"
          >Font size<input
            type="number"
            aria-label="Font size"
            min="4"
            max="512"
            value={node.content.font_size}
            disabled={!editable}
            onchange={(e) => {
              if (node?.content.kind === "text")
                update({
                  ...node,
                  content: {
                    ...node.content,
                    font_size: Math.max(
                      4,
                      Math.min(512, Number(e.currentTarget.value)),
                    ),
                  },
                });
            }}
          /></label
        >
      {:else if node.content.kind === "frame"}<label class="property-stack"
          >Frame name<input
            aria-label="Frame name"
            value={node.content.name}
            disabled={!editable}
            onchange={(e) => {
              if (node?.content.kind === "frame")
                update({
                  ...node,
                  name: e.currentTarget.value,
                  content: { ...node.content, name: e.currentTarget.value },
                });
            }}
          /></label
        >
      {:else if node.content.kind === "shape"}<label class="property-color"
          >Shape<select
            aria-label="Shape kind"
            value={node.content.shape}
            disabled={!editable}
            onchange={(e) => {
              if (node?.content.kind === "shape")
                update({
                  ...node,
                  content: {
                    ...node.content,
                    shape: e.currentTarget.value as
                      "rectangle" | "ellipse" | "line",
                    stroke:
                      e.currentTarget.value === "line"
                        ? node.content.fill
                        : node.content.stroke,
                    stroke_width:
                      e.currentTarget.value === "line"
                        ? 3
                        : node.content.stroke_width,
                  },
                });
            }}
            ><option value="rectangle">Rectangle</option><option value="ellipse"
              >Ellipse</option
            ><option value="line">Line</option></select
          ></label
        >
      {:else if node.content.kind === "image"}<p class="create-hint">
          Crop · original image retained
        </p>
        <div class="property-grid">
          {#each ["x", "y", "width", "height"] as key}<label
              ><span
                >{key === "width"
                  ? "W%"
                  : key === "height"
                    ? "H%"
                    : key.toUpperCase() + "%"}</span
              ><input
                type="number"
                aria-label={`Crop ${key} percent`}
                min="0"
                max="100"
                value={Math.round(
                  { x: 0, y: 0, width: 1, height: 1, ...node.content.crop }[
                    key as "x" | "y" | "width" | "height"
                  ] * 100,
                )}
                disabled={!editable}
                onchange={(e) =>
                  crop(
                    key as "x" | "y" | "width" | "height",
                    Number(e.currentTarget.value),
                  )}
              /></label
            >{/each}
        </div>
        <button
          type="button"
          disabled={!editable}
          onclick={() => {
            if (node?.content.kind === "image")
              update(
                { ...node, content: { ...node.content, crop: null } },
                "Reset crop",
              );
          }}>Reset crop</button
        >{/if}
      {#if node.content.kind === "frame"}<label class="property-color"
          >Transparent<input
            type="checkbox"
            checked={node.content.background === "transparent"}
            disabled={!editable}
            onchange={(e) => {
              if (node?.content.kind === "frame")
                update(
                  {
                    ...node,
                    content: {
                      ...node.content,
                      background: e.currentTarget.checked
                        ? "transparent"
                        : "#f7f5ee",
                    },
                  },
                  "Frame background",
                );
            }}
          /></label
        >{/if}
      <div class="layer-order">
        <button type="button" {disabled} onclick={() => reorder(-1)}
          ><ArrowDown size={13} />Backward</button
        ><button type="button" {disabled} onclick={() => reorder(1)}
          ><ArrowUp size={13} />Forward</button
        >
      </div>
    </div>
  {:else}<p class="create-hint">
      {selected.length > 1 ? "Move or resize these objects together, or group them to keep them together." : "Select an object to edit its properties."}
    </p>{/if}
  <div class="create-layers">
    <h3>Layers</h3>
    {#if renameError}<p class="create-hint" role="alert">{renameError}</p>{/if}
    <div role="tree" aria-label="Canvas layers" aria-multiselectable="true">
      {#each rows as row (row.node.id)}
        {@const layer = row.node}
        {@const Icon = icon(layer)}
        <div class="create-layer" data-layer-id={layer.id} class:selected={selected.includes(layer.id)} class:muted={!layer.visible} role="treeitem" aria-level={row.depth + 1} aria-selected={selected.includes(layer.id)} aria-expanded={row.children ? !collapsed(layer) : undefined} style:padding-left={`${row.depth * 14}px`}>
          {#if row.children}
            <button type="button" class="layer-disclosure" aria-label={`${collapsed(layer) ? "Expand" : "Collapse"} ${nodeName(layer)}`} onclick={() => toggle(layer)}>{#if collapsed(layer)}<ChevronRight size={12}/>{:else}<ChevronDown size={12}/>{/if}</button>
          {:else}<span class="layer-disclosure-spacer"></span>{/if}
          {#if renaming === layer.id}
            <input class="layer-rename" bind:this={renameInput} aria-label="Layer name" maxlength="128" bind:value={renameDraft} onblur={finishRename} onkeydown={(event) => {
              event.stopPropagation();
              if (event.key === "Enter") { event.preventDefault(); finishRename(); }
              if (event.key === "Escape") { event.preventDefault(); renaming = null; renameError = ""; }
            }}/>
          {:else}
            <button type="button" class="layer-name" aria-label={nodeName(layer)} onclick={(event) => selectLayer(layer, event)} ondblclick={() => void beginRename(layer)} onkeydown={(event) => layerKey(layer, event)} title="Double-click to rename · F2"><Icon size={13}/><span>{nodeName(layer)}</span>{#if row.children && collapsed(layer)}<small>{row.children}</small>{/if}</button>
          {/if}
          <div class="layer-actions" class:persistent={!layer.visible || layer.locked}>
            <button type="button" aria-label={layer.visible ? "Hide layer" : "Show layer"} {disabled} onclick={() => update({ ...layer, visible: !layer.visible }, "Toggle visibility")}>{#if layer.visible}<Eye size={12}/>{:else}<EyeOff size={12}/>{/if}</button>
            <button type="button" aria-label={layer.locked ? "Unlock layer" : "Lock layer"} {disabled} onclick={() => update({ ...layer, locked: !layer.locked }, "Toggle lock")}>{#if layer.locked}<Lock size={12}/>{:else}<Unlock size={12}/>{/if}</button>
          </div>
        </div>
      {/each}
    </div>
  </div>
</aside>

<style>
  .create-inspector {
    width: 230px;
    flex: none;
    overflow: auto;
    border-left: 1px solid var(--create-line);
    background: var(--create-bg);
    padding: 14px;
    color: var(--create-text);
    font-size: 12px;
  }
  .create-inspector header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .create-inspector strong {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .create-inspector button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px;
    border-radius: 5px;
  }
  .create-inspector button:hover {
    background: var(--create-soft);
  }
  .create-properties {
    padding-top: 16px;
  }
  .property-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px;
  }
  .property-grid label {
    display: flex;
    align-items: center;
    gap: 5px;
    background: var(--create-soft);
    padding: 5px 7px;
    border-radius: 5px;
  }
  .property-grid span {
    color: var(--create-dim);
    font-size: 10px;
  }
  .create-inspector input:not([type="color"]),
  .create-inspector textarea,
  .create-inspector select {
    min-width: 0;
    width: 100%;
    border: 1px solid var(--create-line);
    background: transparent;
    border-radius: 5px;
    padding: 5px;
    color: inherit;
    font: inherit;
  }
  .property-grid input {
    border: 0 !important;
    padding: 0 !important;
  }
  .property-color {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 14px;
  }
  .property-color input:not([type="color"]),
  .property-color select {
    max-width: 112px;
  }
  .property-color input[type="color"] {
    height: 24px;
    width: 32px;
    padding: 0;
    border: 0;
    background: transparent;
  }
  .property-stack {
    display: grid;
    gap: 7px;
    margin-top: 14px;
    color: var(--create-dim);
  }
  .property-stack textarea {
    min-height: 80px;
    resize: vertical;
  }
  .layer-order {
    display: flex;
    gap: 5px;
    margin-top: 16px;
    font-size: 10px;
    color: var(--create-dim);
  }
  .create-layers {
    border-top: 1px solid var(--create-line);
    margin-top: 20px;
    padding-top: 14px;
  }
  .create-layers h3 {
    font-size: 11px;
    font-weight: 500;
    color: var(--create-dim);
    margin: 0 0 10px;
  }
  .create-layer {
    display: flex;
    align-items: center;
    border-radius: 5px;
    gap: 2px;
    min-height: 32px;
  }
  .create-layer.selected {
    background: var(--create-soft);
  }
  .layer-disclosure, .layer-disclosure-spacer { width: 18px; flex: none; }
  .layer-disclosure { justify-content: center; padding: 3px !important; }
  .layer-rename { flex: 1; width: 0; }
  .layer-actions { display: flex; opacity: 0; }
  .layer-actions.persistent, .create-layer:hover .layer-actions, .create-layer:focus-within .layer-actions, .create-layer.selected .layer-actions { opacity: 1; }
  .layer-name small { margin-left: auto; color: var(--create-dim); font-size: 10px; }
  .create-layer.muted .layer-name { opacity: .5; }
  .create-layer .layer-name {
    flex: 1;
    min-width: 0;
    text-align: left;
  }
  .layer-name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .create-hint {
    font-size: 11px;
    line-height: 1.6;
    color: var(--create-dim);
    margin: 14px 0;
  }
  .create-inspector button:disabled,
  .create-inspector input:disabled,
  .create-inspector textarea:disabled {
    opacity: 0.45;
  }
  @media (max-width: 720px) {
    .create-inspector {
      position: absolute;
      inset: 0 0 0 auto;
      z-index: 15;
      box-shadow: -10px 0 30px #0002;
    }
  }
</style>
