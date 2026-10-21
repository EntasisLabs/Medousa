<script lang="ts">
  import CreateSceneNode from "./CreateSceneNode.svelte";
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  import { inkPath, localMatrix } from "$lib/create/creativeScene";
  let {
    node,
    nodes,
    images,
    selected = [],
    zoom = 1,
    prefix = "create",
    locked = false,
    editingId,
  }: {
    node: CreativeNode;
    nodes: CreativeNode[];
    images: Record<string, string>;
    selected?: string[];
    zoom?: number;
    prefix?: string;
    locked?: boolean;
    editingId?: string;
  } = $props();
  const content = $derived(node.content);
  const w = $derived(node.transform.width),
    h = $derived(node.transform.height);
  const children = $derived(nodes.filter((n) => n.parent_id === node.id));
  const isLocked = $derived(locked || node.locked);
  const clip = $derived(`${prefix}-clip-${node.id}`);
</script>

{#if node.visible}
  <g
    transform={`matrix(${localMatrix(node).join(" ")})`}
    opacity={node.opacity}
    data-node-id={node.id}
    data-locked={isLocked ? "true" : undefined}
  >
    {#if content.kind === "frame"}
      <defs><clipPath id={clip}><rect width={w} height={h} /></clipPath></defs>
      <rect width={w} height={h} fill={content.background} />
      <g clip-path={`url(#${clip})`}
        >{#each children as child (child.id)}<CreateSceneNode
            node={child}
            {nodes}
            {images}
            {selected}
            {zoom}
            {prefix}
            locked={isLocked}
            {editingId}
          />{/each}</g
      >
      <text
        data-editor-only
        x="0"
        y={-12 / zoom}
        font-size={11 / zoom}
        fill="currentColor"
        opacity="0.6"
        pointer-events="none">{content.name}</text
      >
    {:else if content.kind === "group"}
      <rect width={w} height={h} fill="transparent" />
      {#each children as child (child.id)}<CreateSceneNode
          node={child}
          {nodes}
          {images}
          {selected}
          {zoom}
          {prefix}
          locked={isLocked}
          {editingId}
        />{/each}
    {:else if content.kind === "text"}
      <rect width={w} height={h} fill="transparent" />
      <text
        visibility={editingId === node.id ? "hidden" : "visible"}
        fill={content.color}
        font-size={content.font_size}
        font-family={content.font_family}
        dominant-baseline="hanging"
        style="white-space:pre"
        >{#each content.text.split("\n") as line, i}<tspan
            x="0"
            dy={i ? content.font_size * 1.2 : 0}>{line || " "}</tspan
          >{/each}</text
      >
    {:else if content.kind === "shape"}
      <rect width={w} height={h} fill="transparent" />
      {#if content.shape === "ellipse"}<ellipse
          cx={w / 2}
          cy={h / 2}
          rx={w / 2}
          ry={h / 2}
          fill={content.fill}
          stroke={content.stroke}
          stroke-width={content.stroke_width}
        />
      {:else if content.shape === "line"}<line
          x1="0"
          y1="0"
          x2={w}
          y2={h}
          stroke={content.stroke}
          stroke-width={content.stroke_width}
        />
      {:else}<rect
          width={w}
          height={h}
          fill={content.fill}
          stroke={content.stroke}
          stroke-width={content.stroke_width}
        />{/if}
    {:else if content.kind === "ink"}
      <path
        d={inkPath(node)}
        fill={content.color}
        opacity={content.brush.opacity}
      />
    {:else if content.kind === "image"}
      {#if images[content.asset_id]}
        <svg
          width={w}
          height={h}
          viewBox={content.crop
            ? `${content.crop.x} ${content.crop.y} ${content.crop.width} ${content.crop.height}`
            : "0 0 1 1"}
          preserveAspectRatio="none"
          overflow="hidden"
        >
          <image
            data-asset-id={content.asset_id}
            href={images[content.asset_id]}
            width="1"
            height="1"
            preserveAspectRatio="none"
          />
        </svg>
      {:else}<rect
          width={w}
          height={h}
          fill="#88888818"
          stroke="#88888850"
        /><text
          x={w / 2}
          y={h / 2}
          fill="currentColor"
          text-anchor="middle"
          font-size={14 / zoom}>Loading image…</text
        >{/if}
    {/if}
    {#if selected.includes(node.id)}
      <g
        data-editor-only
        pointer-events="none"
        stroke="var(--create-selection, #8e8bd6)"
        fill="none"
        stroke-width={1.5 / zoom}
      >
        <rect width={w} height={h} />
        {#if !isLocked}
          <rect
            x={w - 4 / zoom}
            y={h - 4 / zoom}
            width={8 / zoom}
            height={8 / zoom}
            fill="var(--create-handle,#fff)"
            pointer-events="all"
            data-resize={node.id}
            style="cursor:nwse-resize"
          />
        {/if}
      </g>
    {/if}
  </g>
{/if}
