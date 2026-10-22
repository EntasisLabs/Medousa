<script lang="ts">
  import CreateSceneNode from "./CreateSceneNode.svelte";
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  import { inkPath, localMatrix, nodeName } from "$lib/create/creativeScene";
  import { lineGeometry, linePaint } from "$lib/create/creativeLine";
  import { layoutText, textMetricsVersion } from "$lib/create/creativeText";
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
  const connector = $derived(lineGeometry(node));
  const paint = $derived(connector ? linePaint(connector) : null);
  const textLayout = $derived.by(() => { void $textMetricsVersion; return content.kind === "text" ? layoutText(content) : null; });
  const w = $derived(textLayout?.width ?? node.transform.width),
    h = $derived(textLayout?.height ?? node.transform.height);
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
        pointer-events="none">{nodeName(node)}</text
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
        style={`font-family:${content.font_family};font-size:${content.font_size}px;font-weight:${content.font_weight ?? 400};font-style:${content.font_style ?? "normal"};white-space:pre;letter-spacing:normal;word-spacing:normal;text-transform:none;dominant-baseline:alphabetic;`}
        >{#each textLayout!.lines as line, i}<tspan
            x={textLayout!.insetX}
            y={textLayout!.baseline + i * textLayout!.lineHeight}>{line || " "}</tspan
          >{/each}</text
      >
    {:else if connector && paint}
      <g data-line opacity={connector.opacity}>
        <line x1={paint.start.x} y1={paint.start.y} x2={paint.end.x} y2={paint.end.y} stroke={connector.color} stroke-width={connector.stroke_width} stroke-linecap="round" stroke-dasharray={connector.dashed ? `${connector.stroke_width * 3} ${connector.stroke_width * 2}` : undefined}/>
        {#each paint.heads as triangle}<polygon data-arrowhead points={triangle.map(p => `${p.x},${p.y}`).join(" ")} fill={connector.color}/>{/each}
      </g>
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

  </g>
{/if}
