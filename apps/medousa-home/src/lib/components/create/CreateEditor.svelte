<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    MousePointer2,
    Hand,
    PenTool,
    Type,
    Square,
    Frame,
    ImagePlus,
    PanelLeft,
    Undo2,
    Redo2,
    ChevronDown,
    Minus,
    Plus,
    Download,
    ArrowLeft,
    Pin,
    Eraser,
    Shapes,
    Circle,
    MoveUpRight,
    Sparkles,
    SlidersHorizontal,
    CopyPlus,
    Trash2,
    X,
    Group,
    Ungroup,
  } from "@lucide/svelte";
  import CreateCanvas from "./CreateCanvas.svelte";
  import CreateStylePalette from "./CreateStylePalette.svelte";
  import CreateInspector from "./CreateInspector.svelte";
  import CreateOriginalPreview from "./CreateOriginalPreview.svelte";
  import { editableImportScene } from "$lib/create/creativeReferences";
  import CreateCodeBridge from "./CreateCodeBridge.svelte";
  import CreateAsk from "./CreateAsk.svelte";
  import CreateViewControls from "./CreateViewControls.svelte";
  import CreateAlignmentControls from "./CreateAlignmentControls.svelte";
  import { containingFrame } from "$lib/create/creativeLayout";
  import { reconcileCreativeProposal } from "$lib/create/creativeProposal";
  import type { CreativeProjectLease } from "$lib/create/creativeWorkspace.svelte";
  import {
    applySceneOperations,
    createNode,
    nodeName,
    editableNode,
    selectionRoots,
    type CreativeTool,
  } from "$lib/create/creativeScene";
  import {
    canvasSizes,
    nodeColor,
    nodeFill,
    nodeSize,
    styleNode,
    type CanvasShape,
    type CanvasFill,
    type CanvasSize,
    type CanvasStylePatch,
  } from "$lib/create/creativeInteraction";
  import { lineGeometry } from "$lib/create/creativeLine";
  import { textStyleFor, defaultTextStyle } from "$lib/create/creativeText";
  import type { DrawBrushKind } from "$lib/draw/drawDocument";
  import type { DrawEraserMode } from "$lib/draw/drawInteraction";
  import { creativeAssets } from "$lib/create/creativeAssets";
  import { exportCreativeScene } from "$lib/create/creativeExport";
  import { captureCreativeSelection } from "$lib/create/creativeCapture";
  import type {
    CreativeNode,
    CreativeOperation,
  } from "$lib/types/generated/daemon_api";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { loadCreativePreferences, saveCreativePreferences } from "$lib/create/creativePreferences";
  import { canGroup } from "$lib/create/creativeSelection";
  import { acknowledgeCreativeProject } from "$lib/create/creativeProjectActions";
  let {
    lease,
    onBrowse = () => shellTabs.openSurface("create"),
  }: { lease: CreativeProjectLease; onBrowse?: () => void } = $props();
  const preferences = untrack(() => loadCreativePreferences(lease.api.scope));
  const retainedView = untrack(() => { const reset = !lease.state.referenceViewReady && lease.session.document?.scene.nodes.some(n => n.id === "original_frame"); lease.state.referenceViewReady = true; return reset ? null : lease.state.view; });
  let editorElement = $state<HTMLDivElement>(),
    workareaWidth = $state(0),
    inkColor = $state("#393e33");
  const compactPanels = $derived(workareaWidth > 0 && workareaWidth < 500);
  let version = $state(0),
    canvas = $state<ReturnType<typeof CreateCanvas>>(),
    selected = $state<string[]>(untrack(() => [...lease.state.selected])),
    tool = $state<CreativeTool>(preferences.tool),
    color = $state(preferences.color ?? "#393e33"),
    size = $state<CanvasSize>(preferences.size),
    lineStyles = $state(preferences.lineStyles),
    layout = $state(preferences.layout),
    detailsSections = $state(preferences.detailsSections),
    textStyle = $state({ ...defaultTextStyle, ...preferences.textStyle }),
    brushKind = $state<DrawBrushKind>(preferences.brushKind),
    brushSizes = $state(preferences.brushSizes),
    eraserSize = $state(preferences.eraserSize),
    selectionMode = $state<"box" | "lasso">(preferences.selectionMode),
    eraserMode = $state<DrawEraserMode>(preferences.eraserMode),
    fingerDraw = $state(preferences.fingerDraw),
    shape = $state<CanvasShape>(preferences.shape),
    fill = $state<CanvasFill>(preferences.fill),
    additiveSelect = $state(false), palette = $state(true),
    viewOpen = $state(false),
    shapePicker = $state(false),
    askOpen = $state(false),
    inspector = $state(false),
    error = $state<string | null>(null),
    images = $state.raw<Record<string, string>>({}),
    imageErrors = $state<string | null>(null),
    importing = $state(false),
    exporting = $state(false),
    upload = $state<HTMLInputElement>(),
    zoom = $state(100),
    scopeName = $state<string | null>(null);
  const drawSize = $derived(tool === "erase" ? eraserSize : brushSizes[brushKind]);
  $effect(() => saveCreativePreferences(lease.api.scope, {
    layout: { ...layout },
    tool, selectionMode, brushKind, brushSizes: { ...brushSizes }, eraserSize,
    lineStyles: { line: { ...lineStyles.line }, arrow: { ...lineStyles.arrow } }, detailsSections: { ...detailsSections }, textStyle: { ...textStyle }, eraserMode, fingerDraw, color: color === inkColor ? null : color, size, shape, fill,
  }));
  $effect(() => { if (compactPanels && inspector) palette = false; });
  function togglePalette() {
    const open = !palette;
    if (open && compactPanels) inspector = false;
    if (open) viewOpen = false;
    palette = open;
  }
  $effect(() => { lease.state.selected = [...selected]; });
  const session = $derived(lease.session),
    document = $derived.by(() => {
      void version;
      return session.document;
    });
  $effect(() => {
    const current = document;
    if (current) untrack(() => acknowledgeCreativeProject(lease.api.scope, current));
  });
  const busy = $derived.by(() => {
      void version;
      return session.busy;
    }),
    dirty = $derived.by(() => {
      void version;
      return session.dirty;
    });
  const job = $derived.by(() => {
    void version;
    return lease.state.jobs.job;
  });
  const base = $derived.by(() => {
    void version;
    return session.scene;
  });
  const proposal = $derived(base && job && document ? reconcileCreativeProposal(base, document.revision, job) : { operations: [], error: null });
  const preview = $derived(
    Boolean(
      lease.state.preview &&
      (job?.status === "ready" || job?.status === "stale") &&
      !proposal.error &&
      !dirty,
    ),
  );
  const scene = $derived(
    base && preview && job?.result
      ? editableImportScene(applySceneOperations(base, proposal.operations))
      : base ? editableImportScene(base) : base,
  );
  const deciding = $derived.by(() => {
    void version;
    return lease.state.jobs.pendingDecision !== null;
  });
  const disabled = $derived.by(() => {
    void version;
    return !session.canEdit || preview || importing || deciding;
  });
  const assets = untrack(() =>
    creativeAssets(lease.api, lease.session.document?.id ?? ""),
  );
  // The editor is keyed to its project; acquire() is allowed before its first load.
  const assetId = $derived(document?.id ?? "");
  let assetStore = assets;
  let disposed = false,
    imageEpoch = 0;
  let editingNode = $state.raw<CreativeNode | null>(null);
  const selectedNodes = $derived(
    scene?.nodes.filter((n) => selected.includes(n.id)) ?? [],
  );
  const selectedRoots = $derived(scene ? selectionRoots(selected, scene.nodes) : []);
  const groupAllowed = $derived(scene ? canGroup(selected, scene.nodes) : false);
  const ungroupAllowed = $derived(selectedRoots.some(n => n.content.kind === "group"));
  const styledNode = $derived(
    editingNode ?? (tool === "select" ? selectedNodes.find((n) => nodeColor(n) !== null) : undefined),
  );
  const selectedLine = $derived(tool === "select" && selectedNodes.length > 0 && selectedNodes.every(n => lineGeometry(n)) ? lineGeometry(selectedNodes[0]) : null);
  const currentLineStyle = $derived(shape === "arrow" ? lineStyles.arrow : lineStyles.line);
  const styleColor = $derived(styledNode ? nodeColor(styledNode)! : color);
  const styleSize = $derived(styledNode ? nodeSize(styledNode) : size);
  const styleFill = $derived(
    styledNode?.content.kind === "shape" ? nodeFill(styledNode) : fill,
  );
  const shapeChoices = [
    { id: "rectangle", label: "Rectangle", icon: Square },
    { id: "ellipse", label: "Ellipse", icon: Circle },
    { id: "line", label: "Line", icon: Minus },
    { id: "arrow", label: "Arrow", icon: MoveUpRight },
  ] as const;
  function chooseTool(next: CreativeTool) {
    canvas?.finishText();
    tool = next;
    shapePicker = false;
  }
  function styleSelection(patch: CanvasStylePatch) {
    if (disabled) return;
    if (patch.color) color = patch.color;
    if (patch.size) size = patch.size;
    if (patch.fill) fill = patch.fill;
    if (patch.font_family) textStyle.font_family = patch.font_family;
    if (patch.font_size !== undefined && Number.isFinite(patch.font_size)) textStyle.font_size = Math.max(4, Math.min(512, patch.font_size));
    if (patch.font_weight !== undefined && patch.font_weight !== null) textStyle.font_weight = patch.font_weight;
    if (patch.font_style) textStyle.font_style = patch.font_style;
    for (const key of ["text_width", "text_align", "line_height", "letter_spacing", "paragraph_spacing", "underline"] as const) if (patch[key] !== undefined) textStyle = { ...textStyle, [key]: patch[key] };
    if (patch.stroke_width !== undefined || patch.dashed !== undefined || patch.start_arrow !== undefined || patch.end_arrow !== undefined) {
      const key = tool === "shape" ? shape === "arrow" ? "arrow" : "line" : selectedLine?.end_arrow || selectedLine?.start_arrow ? "arrow" : "line";
      const next = { ...lineStyles[key] };
      if (patch.stroke_width !== undefined) next.stroke_width = patch.stroke_width;
      if (patch.dashed !== undefined) next.dashed = patch.dashed;
      if (patch.start_arrow !== undefined) next.start_arrow = patch.start_arrow;
      if (patch.end_arrow !== undefined) next.end_arrow = patch.end_arrow;
      lineStyles[key] = next;
    }
    if (canvas?.styleText(patch)) return;
    const operations: CreativeOperation[] = (tool === "select" ? selectedNodes : [])
      .filter((n) => scene && editableNode(n, scene.nodes))
      .flatMap((node) => {
        const next = styleNode(node, patch);
        return JSON.stringify(next) === JSON.stringify(node)
          ? []
          : [{ op: "put_node" as const, node: next }];
      });
    if (operations.length) void commit("Style selection", operations);
  }
  const tools = [
    { id: "select", icon: MousePointer2, label: "Select", key: "V" },
    { id: "hand", icon: Hand, label: "Move", key: "H" },
    { id: "ink", icon: PenTool, label: "Draw", key: "P" },
    { id: "erase", icon: Eraser, label: "Erase", key: "E" },
    { id: "shape", icon: Shapes, label: "Shapes", key: "R" },
    { id: "text", icon: Type, label: "Text", key: "T" },
  ] as const;
  function report(e: unknown) {
    error = e instanceof Error ? e.message : String(e);
  }
  async function commit(label: string, operations: CreativeOperation[]) {
    if (!operations.length || !session.canEdit || preview || deciding) return;
    try {
      error = null;
      session.enqueue(label, operations);
      await session.save();
    } catch (e) {
      report(e);
    }
  }
  async function history(redo = false) {
    if (busy || dirty) return;
    try {
      error = null;
      await session.history(redo);
      selected = selected.filter((id) =>
        session.document?.scene.nodes.some((n) => n.id === id),
      );
    } catch (e) {
      report(e);
    }
  }
  async function reload() {
    try {
      error = null;
      await session.load();
    } catch (e) {
      report(e);
    }
  }
  async function retry() {
    try {
      error = null;
      if (session.pending) await session.save();
      else if (session.pendingHistory)
        await session.history(session.pendingHistory.redo);
      else await session.load();
    } catch (e) {
      report(e);
    }
  }
  async function discard() {
    try {
      session.discardPending();
      await reload();
    } catch (e) {
      report(e);
    }
  }
  async function loadImages() {
    if (!scene || !assetId) return;
    const epoch = ++imageEpoch,
      currentStore = assetStore;
    const wanted = new Set(
      scene.nodes.flatMap((n) =>
        n.content.kind === "image" ? [n.content.asset_id] : [],
      ),
    );
    imageErrors = null;
    for (const id of wanted) {
      try {
        const url = await currentStore.get(id);
        if (disposed || epoch !== imageEpoch) return;
        images = { ...images, [id]: url };
      } catch (e) {
        if (!disposed && epoch === imageEpoch)
          imageErrors = e instanceof Error ? e.message : String(e);
      }
    }
    if (!disposed && epoch === imageEpoch) {
      images = Object.fromEntries(
        Object.entries(images).filter(([id]) => wanted.has(id)),
      );
      assetStore.retainOnly(wanted);
    }
  }
  $effect(() => {
    if (assetId) {
      assetStore.dispose();
      assetStore = creativeAssets(lease.api, assetId);
      images = {};
    }
  });
  $effect(() => {
    const current = scene,
      id = assetId;
    if (current && id) untrack(() => void loadImages());
  });
  async function importImage(file?: File) {
    if (!file || !document || disabled || busy || dirty) return;
    if (!["image/png", "image/jpeg", "image/webp"].includes(file.type)) {
      error = "Choose a PNG, JPEG, or WebP image";
      return;
    }
    if (file.size > 25 * 1024 * 1024) {
      error = "Choose an image under 25 MiB";
      return;
    }
    importing = true;
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      for (let i = 0; i < bytes.length; i += 32768)
        binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
      const asset = await lease.api.importAsset(document.id, btoa(binary));
      await session.load();
      const url = await assetStore.get(asset.id),
        img = new Image();
      img.src = url;
      await img.decode();
      const parent =
        document.scene.nodes.find(
          (n) =>
            selected.includes(n.id) &&
            (n.content.kind === "frame" || n.content.kind === "group") &&
            !n.locked,
        ) ??
        document.scene.nodes.find(
          (n) => n.content.kind === "frame" && !n.locked,
        );
      const maxWidth = parent?.transform.width ?? 1200,
        maxHeight = parent?.transform.height ?? 920,
        scale = Math.min(
          1,
          maxWidth / img.naturalWidth,
          maxHeight / img.naturalHeight,
        ),
        width = img.naturalWidth * scale,
        height = img.naturalHeight * scale;
      const node = createNode(
        { kind: "image", asset_id: asset.id, crop: null },
        {
          x: (maxWidth - width) / 2,
          y: (maxHeight - height) / 2,
          width,
          height,
        },
        parent?.id ?? null,
      );
      await commit("Import image", [{ op: "put_node", node }]);
      selected = [node.id];
      images = { ...images, [asset.id]: url };
    } catch (e) {
      report(e);
    } finally {
      importing = false;
      if (upload) upload.value = "";
    }
  }
  async function exportImage(format: "png" | "svg") {
    if (!document || !canvas?.element() || busy || dirty) return;
    exporting = true;
    try {
      await loadImages();
      await exportCreativeScene(
        canvas.element()!,
        scene?.nodes ?? document.scene.nodes,
        assetStore,
        document.title,
        format,
        selected.find(
          (id) =>
            document?.scene.nodes.find((n) => n.id === id)?.content.kind ===
            "frame",
        ),
      );
    } catch (e) {
      report(e);
    } finally {
      exporting = false;
    }
  }
  async function captureSelection(revision: number, ids: string[]) {
    if (!document || document.revision !== revision || busy || dirty || !canvas?.element())
      throw new Error("Finish saving the canvas before capturing your selection");
    await loadImages();
    if (!scene || document.revision !== revision || busy || dirty)
      throw new Error("The canvas changed. Capture the selection again");
    const data = await captureCreativeSelection(canvas.element()!, scene.nodes, ids, assetStore);
    const asset = await lease.api.captureSelection(document.id, data, { expected_revision: revision, selected_ids: ids });
    return asset.id;
  }
  function key(e: KeyboardEvent) {
    if (
      (e.target as HTMLElement).closest(
        "input,textarea,select,[contenteditable=true]",
      )
    )
      return;
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "g") {
      e.preventDefault(); if (e.shiftKey) canvas?.ungroup(); else canvas?.group(); return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (!disabled) void history(e.shiftKey);
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "Escape") {
      shapePicker = false;
      askOpen = false;
      tool = "select";
      return;
    }
    if (e.key.toLowerCase() === "f") {
      chooseTool("frame");
      e.preventDefault();
      return;
    }
    const next = tools.find((t) => t.key.toLowerCase() === e.key.toLowerCase());
    if (next) {
      chooseTool(next.id);
      e.preventDefault();
    }
  }
  onMount(() => {
    const rgb =
      editorElement &&
      getComputedStyle(editorElement).color.match(
        /^(?:rgb|rgba)\((\d+)[, ]+(\d+)[, ]+(\d+)/,
      );
    if (rgb) {
      inkColor =
        "#" +
        rgb
          .slice(1, 4)
          .map((n) => Number(n).toString(16).padStart(2, "0"))
          .join("");
      if (preferences.color === null) color = inkColor;
    }
    if (window.matchMedia("(max-width: 600px)").matches) palette = false;
    const a = session.subscribe(() => version++),
      b = lease.state.jobs.subscribe(() => version++);
    if (!session.document && !session.busy) void reload();
    return () => {
      disposed = true;
      a();
      b();
      assetStore.dispose();
    };
  });
</script>

<div
  bind:this={editorElement}
  class="create-editor"
  onkeydown={key}
  ondragover={(event) => {
    if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
  }}
  ondrop={(event) => {
    if (event.dataTransfer?.files.length) {
      event.preventDefault();
      void importImage(event.dataTransfer.files[0]);
    }
  }}
  onpaste={(event) => {
    if (
      (event.target as HTMLElement).closest(
        "input,textarea,select,[contenteditable=true]",
      )
    )
      return;
    const image = [...(event.clipboardData?.files ?? [])].find((file) =>
      file.type.startsWith("image/"),
    );
    if (image) {
      event.preventDefault();
      void importImage(image);
    }
  }}
  role="presentation"
>
  {#if document && scene}
    <header class="create-heading">
      <div class="create-breadcrumb">
        <button
          type="button"
          aria-label="Back to creative projects"
          onclick={onBrowse}
          ><ArrowLeft size={14} class="create-back-icon" /><span
            class="create-browse-label">Create</span
          ></button
        ><span>/</span><input
          aria-label="Project title"
          value={document.title}
          {disabled}
          maxlength="256"
          onchange={(e) => {
            const title = e.currentTarget.value.trim();
            if (title && title !== document?.title)
              void commit("Rename project", [{ op: "rename", title }]);
          }}
        />
      </div>
      <button
        type="button"
        aria-label={document.pinned ? "Unpin project" : "Pin project"}
        aria-pressed={document.pinned}
        {disabled}
        onclick={() =>
          void commit("Pin project", [
            { op: "pin", pinned: !document!.pinned },
          ])}><Pin size={13} /></button
      >
      <span class="create-save"
        >{busy
          ? "Saving…"
          : dirty
            ? "Unsaved edit"
            : preview
              ? "Preview"
              : "Saved"}</span
      >
      <button
        type="button"
        title="Undo (⌘Z)"
        aria-label="Undo"
        disabled={disabled || busy || dirty || !session.canUndo}
        onclick={() => void history()}><Undo2 size={15} /></button
      ><button
        type="button"
        title="Redo (⇧⌘Z)"
        aria-label="Redo"
        disabled={disabled || busy || dirty || !session.canRedo}
        onclick={() => void history(true)}><Redo2 size={15} /></button
      >
      <CreateOriginalPreview {lease} {document} {selected} onframe={id => { canvas?.finishText(); canvas?.focusFrame(id); }}/>
      <CreateCodeBridge {lease} {document} frame={selected.length === 1 ? scene?.nodes.find(n => n.id === selected[0] && n.content.kind === "frame") : undefined} disabled={disabled || busy || dirty || Boolean(editingNode)} capture={captureSelection}/>
      <details class="create-export">
        <summary><Download size={13} />Export<ChevronDown size={11} /></summary>
        <div>
          <button
            type="button"
            disabled={exporting || disabled || busy || dirty}
            onclick={() => void exportImage("png")}>PNG image</button
          ><button
            type="button"
            disabled={exporting || disabled || busy || dirty}
            onclick={() => void exportImage("svg")}>SVG document</button
          >
        </div>
      </details>
    </header>
    {#if error || (dirty && !busy)}<div class="create-error" role="alert">
        <span
          >{error || "An edit needs your attention before you continue."}</span
        ><button type="button" disabled={busy} onclick={() => void retry()}
          >Retry</button
        >{#if dirty}<button
            type="button"
            disabled={busy}
            onclick={() => void discard()}>Discard draft and reload</button
          >{:else}<button type="button" onclick={() => (error = null)}
            >Dismiss</button
          >{/if}
      </div>{/if}
    {#if imageErrors}<div class="create-error" role="status">
        <span>{imageErrors}</span><button
          type="button"
          onclick={() => void loadImages()}>Retry images</button
        >
      </div>{/if}
    <div class="create-editor-body">
      <main class="create-workarea" bind:clientWidth={workareaWidth}>
        <div class="create-canvas-controls">
          <div class="create-details-controls">
            <button type="button" class="create-details-toggle" aria-label="Toggle inspector" aria-pressed={inspector}
              onclick={() => (inspector = !inspector)}><PanelLeft size={13} />Details</button>
          <span aria-live="polite">
            {#if scopeName}<button type="button" aria-label="Exit group" onclick={() => canvas?.exitGroup()}><ArrowLeft size={12}/>{scopeName}</button>{:else}{preview
              ? "Preview · original preserved"
              : selected.length && tool === "select"
                ? `${selected.length} selected`
                : tool === "text"
                  ? "Click to type"
                  : tool === "shape"
                    ? `${shapeChoices.find((s) => s.id === shape)?.label} · drag to draw`
                    : tool === "ink"
                      ? "Draw freely"
                      : tool === "frame"
                        ? "Drag to add a frame"
                        : tool === "erase"
                          ? eraserMode === "partial"
                            ? "Erase ink"
                            : "Erase whole objects"
                          : scene.nodes.length ? "" : "Draw, type, or drop an image to start"}{/if}</span
          >
          </div>
          <div class="create-style-controls">
            <CreateViewControls bind:settings={layout} bind:open={viewOpen} onopen={() => (palette = false)} onfit={() => canvas?.fit()} onselection={() => canvas?.fitSelection()} hasSelection={selectedRoots.length > 0}/>
            <button
              type="button"
              aria-label="Show style palette"
              class="create-style-toggle"
              aria-expanded={palette}
              onclick={togglePalette}
              ><span class="create-current-color" style:background={styleColor}
              ></span>Style<SlidersHorizontal size={13} /></button
            >
          </div>
        </div>
        {#if inspector}<CreateInspector
          nodes={scene.nodes}
          {selected}
          {disabled}
          bind:propertiesOpen={detailsSections.properties}
          bind:layersOpen={detailsSections.layers}
          onselect={(ids) => (selected = ids)}
          oncommit={(label, ops) => void commit(label, ops)}
          ontextstyle={styleSelection}
          onlinestyle={styleSelection}
          ontextleave={() => canvas?.finishText()}
          editingTextStyle={editingNode?.content.kind === "text" ? textStyleFor(editingNode) : undefined}
          onclose={() => (inspector = false)}
        />{/if}
        <CreateCanvas
          bind:this={canvas}
          {scene}
          {images}
          bind:selected
          bind:tool
          {color}
          automaticInk={color === inkColor}
          {shape}
          lineStyle={currentLineStyle}
          {fill}
          textSize={size}
          {textStyle}
          {brushKind}
          {eraserMode}
          {selectionMode}
          {additiveSelect}
          {layout}
          initialView={retainedView}
          onview={(view) => { lease.state.view = view; }}
          {fingerDraw}
          brushSize={tool === "ink" || tool === "erase"
            ? drawSize
            : canvasSizes[size].stroke}
          {disabled}
          onselect={() => (shapePicker = false)}
          onzoom={(value) => (zoom = value)}
          oncommit={(label, ops) => void commit(label, ops)}
          onerror={report}
          onediting={(node) => (editingNode = node)}
          onScope={(name) => (scopeName = name)}
        />
        {#if palette && tool !== "hand"}<div class="create-palette-position">
            <CreateStylePalette
              node={tool === "select" && selectedNodes.length === 1 ? selectedNodes[0] : undefined}
              {tool}
              {brushKind}
              {drawSize}
              {eraserMode}
              {selectionMode}
              onSelectionMode={(mode) => (selectionMode = mode)}
              {fingerDraw}
              onDrawOptions={(patch) => {
                if (patch.brush) {
                  brushKind = patch.brush;
                }
                if (patch.size) {
                  if (tool === "erase") eraserSize = patch.size;
                  else brushSizes[brushKind] = patch.size;
                }
                if (patch.eraser) eraserMode = patch.eraser;
                if (patch.finger !== undefined) fingerDraw = patch.finger;
              }}
              lineStyle={selectedLine ?? (tool === "shape" && (shape === "line" || shape === "arrow") ? currentLineStyle : undefined)}
              onLineStyle={styleSelection}
              textStyle={editingNode?.content.kind === "text" ? textStyleFor(editingNode) : tool === "text" ? textStyle : tool === "select" && selectedNodes.length > 0 && selectedNodes.every(n => n.content.kind === "text") && selectedNodes[0].content.kind === "text" ? textStyleFor(selectedNodes[0]) : undefined}
              onTextStyle={styleSelection}
              onTextLeave={() => canvas?.finishText()}
              {inkColor}
              showPaint={tool !== "erase" && (tool !== "select" || Boolean(styledNode))}
              showSize={tool !== "select" || Boolean(styledNode && ["text", "ink", "shape"].includes(styledNode.content.kind))}
              color={styleColor}
              size={styleSize}
              fill={styleFill}
              context={selectedRoots.length > 1 ? `${selectedRoots.length} selected` : styledNode ? nodeName(styledNode) : selectedRoots.length === 1 ? nodeName(selectedRoots[0]) : tool === "select" ? "Selection" : "Next object"}
              {disabled}
              showFill={tool === "shape" && shape !== "line" && shape !== "arrow" ||
                selectedNodes.some(
                  (n) =>
                    n.content.kind === "shape" && n.content.shape !== "line",
                )}
              onchange={styleSelection}
              onclose={() => (palette = false)}
            />
          </div>{/if}
        <div
          class="create-zoom"
          class:has-selection={selected.length > 0}
          class:has-popover={shapePicker || askOpen}
        >
          <button
            type="button"
            aria-label="Zoom out"
            onclick={() => canvas?.zoomBy(0.8)}><Minus size={12} /></button
          >
          <button
            type="button"
            aria-label="Fit canvas"
            onclick={() => canvas?.fit()}>{zoom}%</button
          >
          <button
            type="button"
            aria-label="Zoom in"
            onclick={() => canvas?.zoomBy(1.25)}><Plus size={12} /></button
          >
        </div>
        <div class="create-bottom">
          {#if askOpen}<div class="create-ask-popover">
              <header>
                <span>Ask Medousa</span><button
                  type="button"
                  aria-label="Close Ask Medousa"
                  onclick={() => (askOpen = false)}><X size={14} /></button
                >
              </header>
              <CreateAsk
                {lease}
                {selected}
                capture={captureSelection}
                previewError={proposal.error}
                disabled={busy || dirty || importing}
                onerror={report}
              />
            </div>{/if}
          {#if selected.length && tool === "select" && !askOpen}<div
              class="create-selection-actions"
            >
              <span>{selectedRoots.length === 1 && selectedRoots[0].content.kind === "group" ? nodeName(selectedRoots[0]) : `${selectedRoots.length} selected`}</span>
              {#if selectedRoots.length > 1}<button type="button" aria-label="Group selection" title="Group (⌘G)" disabled={disabled || !groupAllowed} onclick={() => canvas?.group()}><Group size={15}/></button>{/if}
              {#if ungroupAllowed}<button type="button" aria-label="Ungroup selection" title="Ungroup (⇧⌘G)" {disabled} onclick={() => canvas?.ungroup()}><Ungroup size={15}/></button>{/if}
              <CreateAlignmentControls count={selectedRoots.length} hasFrame={Boolean(containingFrame(selected, scene.nodes))} {disabled} onalign={(alignment, target) => canvas?.align(alignment, target)} ondistribute={(axis) => canvas?.distribute(axis)}/>
              <details class="create-selection-menu">
                <summary aria-label="Arrange selection">Arrange<ChevronDown size={11}/></summary>
                <div role="group" aria-label="Layer order">
                  <button type="button" {disabled} onclick={(event) => { canvas?.arrange(1); event.currentTarget.closest("details")?.removeAttribute("open"); }}>Bring forward</button>
                  <button type="button" {disabled} onclick={(event) => { canvas?.arrange(-1); event.currentTarget.closest("details")?.removeAttribute("open"); }}>Send backward</button>
                </div>
              </details>
              <button
                type="button"
                aria-label="Duplicate selection"
                {disabled}
                onclick={() => canvas?.duplicate()}
                ><CopyPlus size={15} /></button
              >
              <button
                type="button"
                aria-label="Delete selection"
                {disabled}
                onclick={() => canvas?.deleteSelection()}
                ><Trash2 size={15} /></button
              >
              <button
                type="button"
                aria-label="Toggle inspector"
                aria-pressed={inspector}
                onclick={() => (inspector = !inspector)}
                ><SlidersHorizontal size={15} /></button
              >
            </div>{/if}
          {#if shapePicker}<div
              class="create-shape-picker"
              role="group"
              aria-label="Choose a shape"
            >
              {#each shapeChoices as choice}<button
                  type="button"
                  aria-pressed={shape === choice.id && tool === "shape"}
                  {disabled}
                  onclick={() => {
                    shape = choice.id;
                    chooseTool("shape");
                  }}
                  ><choice.icon size={17} /><span>{choice.label}</span></button
                >{/each}
              <button
                type="button"
                {disabled}
                class="create-frame-choice"
                onclick={() => chooseTool("frame")}
                ><Frame size={17} /><span>Frame</span><small>F</small></button
              >
            </div>{/if}
          <div
            class="create-toolstrip"
            role="toolbar"
            aria-label="Canvas tools"
          >
            {#each tools as item}<button
                type="button"
                class:chosen={tool === item.id}
                aria-label={item.label}
                aria-pressed={tool === item.id}
                aria-expanded={item.id === "shape" ? shapePicker : undefined}
                title={`${item.label} (${item.key})`}
                disabled={disabled && item.id !== "hand"}
                onclick={() => {
                  if (item.id === "select" && tool === "select") {
                    palette = true;
                    viewOpen = false;
                  } else if (item.id === "shape") {
                    const open = !shapePicker;
                    chooseTool("shape");
                    shapePicker = open;
                  } else chooseTool(item.id);
                }}><item.icon size={16} /><span>{item.label}</span></button
              >{/each}
            {#if tool === "select"}<button type="button" aria-label="Add to selection" aria-pressed={additiveSelect} title="Add or remove objects from selection" onclick={() => additiveSelect = !additiveSelect}><Plus size={16}/><span>Multi</span></button>{/if}
            <button
              type="button"
              aria-label="Import image"
              disabled={disabled || busy || dirty}
              onclick={() => upload?.click()}
              ><ImagePlus size={16} /><span>Image</span></button
            >
            <button
              type="button"
              class="create-ask-toggle"
              aria-label="Ask Medousa"
              aria-expanded={askOpen}
              onclick={() => {
                canvas?.finishText();
                shapePicker = false;
                askOpen = !askOpen;
              }}
              ><Sparkles size={16} /><span>Ask Medousa</span
              >{#if job && (job.status === "ready" || job.status === "running" || job.status === "queued" || job.status === "dispatching")}<span
                  class="create-job-dot"
                  aria-label={job.status === "ready"
                    ? "Proposal ready"
                    : "Request in progress"}
                ></span>{/if}</button
            >
          </div>
        </div>
        <footer class="create-canvas-footer">
          <span
            >{editingNode
              ? "Type here · Esc to finish"
              : tool === "select"
                ? scopeName ? "Esc exits group" : selectedRoots.length > 1 ? "Shift adds · ⌘G groups · Option-drag duplicates" : selectedLine ? "Drag endpoints · Shift snaps 45° · Drag the round handle to rotate" : selectedRoots.length ? "Drag the round handle to rotate · Shift snaps 15°" : ""
                : tool === "hand"
                  ? "Drag to move · Space temporarily pans"
                  : tool === "ink"
                    ? "Draw freely · Esc returns to Select"
                    : tool === "erase"
                      ? `${eraserMode === "partial" ? "Erase ink" : "Erase whole objects"} · Esc returns to Select`
                      : tool === "text"
                        ? "Click to type"
                        : "Drag to draw · release to select"}</span
          >
        </footer>
        <input
          bind:this={upload}
          type="file"
          accept="image/png,image/jpeg,image/webp"
          hidden
          onchange={(e) => void importImage(e.currentTarget.files?.[0])}
        />
      </main>

    </div>
  {:else}<div class="create-loading">
      {#if error}<p role="alert">{error}</p>
        <button type="button" onclick={() => void reload()}>Try again</button
        >{:else}Opening canvas…{/if}
    </div>{/if}
</div>
