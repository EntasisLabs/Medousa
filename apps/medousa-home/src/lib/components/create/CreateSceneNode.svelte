<script lang="ts">
  import { cornerRadius } from "$lib/create/creativeAppearance";
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
  const textLayout = $derived.by(() => { void $textMetricsVersion; return content.kind === "text" ? layoutText(content, undefined, node.text_layout) : null; });
  const w = $derived(textLayout?.width ?? node.transform.width),
    h = $derived(textLayout?.height ?? node.transform.height);
  const children = $derived(nodes.filter((n) => n.parent_id === node.id));
  const isLocked = $derived(locked || node.locked);
  const radius = $derived(cornerRadius(node));
  const shadow = $derived(node.appearance?.shadow);
  const shadowId = $derived(`${prefix}-shadow-${node.id}`);
  const shadowPad = $derived(shadow ? shadow.blur * 4 + Math.max(Math.abs(shadow.x), Math.abs(shadow.y)) + (content.kind === "shape" ? content.stroke_width / 2 : 0) + 2 : 0);
  const clip = $derived(`${prefix}-clip-${node.id}`);
</script>

{#if node.visible}
  <g
    transform={`matrix(${localMatrix(node).join(" ")})`}
    opacity={node.opacity}
    data-node-id={node.id}
    data-locked={isLocked ? "true" : undefined}
  >
    {#if shadow}
      <defs><filter id={shadowId} filterUnits="userSpaceOnUse" x={-shadowPad} y={-shadowPad} width={w + shadowPad * 2} height={h + shadowPad * 2} color-interpolation-filters="sRGB">
        <feDropShadow dx={shadow.x} dy={shadow.y} stdDeviation={shadow.blur} flood-color={shadow.color} flood-opacity={shadow.opacity}/>
      </filter></defs>
    {/if}
    {#if content.kind === "frame"}
      <defs><clipPath id={clip}><rect width={w} height={h} rx={radius}/></clipPath></defs>
      <rect width={w} height={h} rx={radius} fill={content.background} filter={shadow ? `url(#${shadowId})` : undefined} />
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
            x={textLayout!.align === "center" ? w / 2 : textLayout!.align === "right" ? w - textLayout!.padding : textLayout!.insetX}
            text-anchor={textLayout!.align === "center" ? "middle" : textLayout!.align === "right" ? "end" : "start"}
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
          filter={shadow ? `url(#${shadowId})` : undefined}
        />
      {:else}<rect
          rx={radius}
          width={w}
          height={h}
          fill={content.fill}
          stroke={content.stroke}
          stroke-width={content.stroke_width}
          filter={shadow ? `url(#${shadowId})` : undefined}
        />{/if}
    {:else if content.kind === "ink"}
      <path
        d={inkPath(node)}
        fill={content.color}
        opacity={content.brush.opacity}
      />
    {:else if content.kind === "image"}
      {#if images[content.asset_id]}
        <defs><clipPath id={clip}><rect width={w} height={h} rx={radius}/></clipPath></defs>
        <g filter={shadow ? `url(#${shadowId})` : undefined}><g clip-path={`url(#${clip})`}>
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
        </svg></g></g>
      {:else}<rect
          rx={radius}
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
