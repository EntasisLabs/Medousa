<script lang="ts">
  import "$lib/styles/files-surface.postcss";
  import { AlertCircle, ChevronDown, ChevronLeft, ChevronRight, ExternalLink, LoaderCircle } from "@lucide/svelte";
  import type { PDFDocumentProxy, PDFDocumentLoadingTask, RenderTask } from "pdfjs-dist";
  import { readExternalFile } from "$lib/utils/externalDeskApi";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { pdfRenderer } from "./pdfRenderer";

  let { path }: { path: string } = $props();
  const textId = $props.id();
  let document = $state<PDFDocumentProxy | null>(null);
  let page = $state(1);
  let zoom = $state("page");
  let width = $state(0);
  let height = $state(0);
  let canvas = $state<HTMLCanvasElement>();
  let pageText = $state("");
  let error = $state<string | null>(null);

  $effect(() => {
    const current = path;
    let cancelled = false;
    let loading: PDFDocumentLoadingTask | undefined;
    document = null; page = 1; error = null;
    void Promise.all([readExternalFile(current), pdfRenderer()]).then(async ([payload, pdf]) => {
      if (cancelled) return;
      if (payload.kind !== "base64") throw new Error("This file does not contain PDF bytes.");
      loading = pdf.getDocument({
        data: Uint8Array.from(atob(payload.content), (char) => char.charCodeAt(0)),
        useWorkerFetch: false,
        BinaryDataFactory: pdf.BinaryDataFactory,
      });
      const loaded = await loading.promise;
      if (!cancelled) document = loaded;
    }).catch((reason) => { if (!cancelled) error = reason instanceof Error ? reason.message : String(reason); });
    return () => { cancelled = true; void loading?.destroy(); };
  });

  $effect(() => {
    const pdf = document, target = canvas, available = width, availableHeight = height, number = page, selectedZoom = zoom;
    if (!pdf || !target || !available || !availableHeight) return;
    let cancelled = false;
    let render: RenderTask | undefined;
    pageText = "";
    void pdf.getPage(number).then(async (pdfPage) => {
      if (cancelled) return;
      const base = pdfPage.getViewport({ scale: 1 });
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      const fitWidth = Math.max(1, available - 50) / base.width;
      const desired = selectedZoom === "page" ? Math.min(fitWidth, Math.max(1, availableHeight - 50) / base.height, 2) : selectedZoom === "fit" ? Math.min(fitWidth, 2) : Number(selectedZoom);
      // Bound the single live page canvas, even for unusually large PDF pages.
      const scale = Math.max(.05, Math.min(desired, Math.sqrt(16_000_000 / (base.width * base.height)) / ratio));
      const viewport = pdfPage.getViewport({ scale });
      target.width = Math.ceil(viewport.width * ratio);
      target.height = Math.ceil(viewport.height * ratio);
      target.style.width = `${viewport.width}px`;
      target.style.height = `${viewport.height}px`;
      render = pdfPage.render({ canvas: target, viewport, transform: [ratio, 0, 0, ratio, 0, 0] });
      const text = await pdfPage.getTextContent();
      if (!cancelled) pageText = text.items.map((item) => "str" in item ? item.str : "").join(" ");
      await render.promise;
      if (!cancelled) pdfPage.cleanup();
    }).catch((reason) => { if (!cancelled) error = reason instanceof Error ? reason.message : String(reason); });
    return () => { cancelled = true; render?.cancel(); target.width = target.height = 0; };
  });
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="files-document-toolbar">
    <div class="files-pdf-navigation">
      <button type="button" aria-label="Previous page" title="Previous page" disabled={!document || page <= 1} onclick={() => page--} class="files-icon-button"><ChevronLeft size={14} /></button>
      <span aria-live="polite">{document ? `Page ${page} of ${document.numPages}` : 'Opening PDF…'}</span>
      <button type="button" aria-label="Next page" title="Next page" disabled={!document || page >= document.numPages} onclick={() => page++} class="files-icon-button"><ChevronRight size={14} /></button>
    </div>
    <span class="files-select-wrap"><select aria-label="PDF zoom" bind:value={zoom} class="files-select">
      <option value="page">Fit page</option><option value="fit">Fit width</option><option value="0.5">50%</option><option value="1">100%</option><option value="1.5">150%</option><option value="2">200%</option>
    </select><ChevronDown size={12} /></span>
  </div>
  {#if error}
    <div class="files-empty"><span class="files-empty-icon"><AlertCircle size={22} /></span><h3>Couldn't preview this PDF</h3><p role="alert">{error}</p><button type="button" class="files-button" onclick={() => void openAttachmentPath(path)}><ExternalLink size={13} />Open in app</button></div>
  {/if}
  <div class="files-pdf-stage" bind:clientWidth={width} bind:clientHeight={height} role="region" aria-label="PDF document" class:hidden={!!error}>
    {#if !document && !error}<div class="files-empty" role="status"><LoaderCircle size={20} class="animate-spin" /><p>Opening PDF…</p></div>{/if}
    <canvas bind:this={canvas} style:display={document ? 'block' : 'none'} aria-label={`PDF page ${page}`} aria-describedby={textId}></canvas>
    <p id={textId} class="sr-only">{pageText}</p>
  </div>
</div>
