import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

// Vite emits these as local assets. Only the PDF's requested resources are read.
const resources = import.meta.glob<string>([
  "/node_modules/pdfjs-dist/cmaps/*.bcmap",
  "/node_modules/pdfjs-dist/standard_fonts/*.{pfb,ttf}",
  "/node_modules/pdfjs-dist/wasm/*.{wasm,js}",
], { query: "?url", import: "default", eager: true });

class LocalPdfResources {
  async fetch({ kind, filename }: { kind: string; filename: string }) {
    const folders: Record<string, string> = { cMapUrl: "cmaps", standardFontDataUrl: "standard_fonts", wasmUrl: "wasm" };
    const folder = folders[kind];
    const url = resources[`/node_modules/pdfjs-dist/${folder}/${filename}`];
    if (!url) throw new Error(`PDF resource unavailable: ${filename}`);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`PDF resource could not be loaded: ${filename}`);
    return new Uint8Array(await response.arrayBuffer());
  }
}

export async function pdfRenderer() {
  const pdf = await import("pdfjs-dist");
  pdf.GlobalWorkerOptions.workerSrc = workerUrl;
  return { ...pdf, BinaryDataFactory: LocalPdfResources };
}
