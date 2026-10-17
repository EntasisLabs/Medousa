<script lang="ts">
  import type { PDFDocumentProxy, PDFDocumentLoadingTask, RenderTask } from "pdfjs-dist";
  import { readExternalFile } from "$lib/utils/externalDeskApi";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { pdfRenderer } from "./pdfRenderer";

  let { path }: { path: string } = $props();
  const textId = $props.id();
  let document = $state<PDFDocumentProxy | null>(null);
  let page = $state(1);
  let zoom = $state("fit");
  let width = $state(0);
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
    const pdf = document, target = canvas, available = width, number = page, selectedZoom = zoom;
    if (!pdf || !target || !available) return;
    let cancelled = false;
    let render: RenderTask | undefined;
    pageText = "";
    void pdf.getPage(number).then(async (pdfPage) => {
      if (cancelled) return;
      const base = pdfPage.getViewport({ scale: 1 });
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      const desired = selectedZoom === "fit" ? Math.min((available - 32) / base.width, 2) : Number(selectedZoom);
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
  <div class="flex shrink-0 items-center justify-center gap-3 border-b border-surface-600/30 px-3 py-2 text-chrome-sm">
    <button type="button" disabled={!document || page <= 1} onclick={() => page--} class="rounded px-2 py-1 hover:bg-surface-700/40 disabled:opacity-40">Previous page</button>
    <span aria-live="polite">{document ? `Page ${page} of ${document.numPages}` : 'Loading PDF…'}</span>
    <button type="button" disabled={!document || page >= document.numPages} onclick={() => page++} class="rounded px-2 py-1 hover:bg-surface-700/40 disabled:opacity-40">Next page</button>
    <select aria-label="PDF zoom" bind:value={zoom} class="rounded bg-surface-800 px-2 py-1">
      <option value="fit">Fit width</option><option value="0.5">50%</option><option value="1">100%</option><option value="1.5">150%</option><option value="2">200%</option>
    </select>
  </div>
  {#if error}
    <div class="p-6 text-center text-sm"><p role="alert">{error}</p><button type="button" class="mt-3 rounded border border-surface-600 px-3 py-2" onclick={() => void openAttachmentPath(path)}>Open in app</button></div>
  {/if}
  <div class="min-h-0 flex-1 overflow-auto p-4" bind:clientWidth={width} role="region" aria-label="PDF document">
    <canvas bind:this={canvas} class="mx-auto block bg-white shadow-lg" aria-label={`PDF page ${page}`} aria-describedby={textId}></canvas>
    <p id={textId} class="sr-only">{pageText}</p>
  </div>
</div>
