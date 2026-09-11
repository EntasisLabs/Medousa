import { uploadMediaBytes } from "$lib/daemon";
import { drawStrokeOutlinePath } from "$lib/draw/drawGeometry";
import {
  parseDrawDocumentJson,
  serializeDrawDocumentJson,
  type DrawDocument,
} from "$lib/draw/drawDocument";
import type { MediaRef } from "$lib/types/media";

export const DRAW_MEDIA_MIME = "application/vnd.medousa.draw+json";

function escapeXml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&apos;",
  })[char] ?? char);
}

export function drawDocumentSvg(drawDocument: DrawDocument): string {
  const background = drawDocument.background === "transparent" ? "#0c0c14" : drawDocument.background;
  const paths = drawDocument.strokes.map((stroke) => {
    const path = drawStrokeOutlinePath(stroke);
    return `<path d="${escapeXml(path)}" fill="${escapeXml(stroke.color)}" fill-opacity="${stroke.brush.opacity}"/>`;
  }).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${drawDocument.width} ${drawDocument.height}" width="${drawDocument.width}" height="${drawDocument.height}"><rect width="100%" height="100%" fill="${escapeXml(background)}"/>${paths}</svg>`;
}

export async function drawDocumentPng(drawDocument: DrawDocument): Promise<Uint8Array> {
  const svg = drawDocumentSvg(drawDocument);
  const svgUrl = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  try {
    const image = new Image();
    image.decoding = "async";
    const loaded = new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error("Couldn’t render the drawing preview"));
    });
    image.src = svgUrl;
    await loaded;
    const maxSide = 1600;
    const scale = Math.min(1, maxSide / Math.max(drawDocument.width, drawDocument.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(drawDocument.width * scale));
    canvas.height = Math.max(1, Math.round(drawDocument.height * scale));
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas rendering is unavailable");
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const blob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob((value: Blob | null) => value ? resolve(value) : reject(new Error("Couldn’t encode the drawing preview")), "image/png");
    });
    return new Uint8Array(await blob.arrayBuffer());
  } finally {
    URL.revokeObjectURL(svgUrl);
  }
}

export async function uploadDrawingMedia(
  sessionId: string,
  document: DrawDocument,
  label = "Drawing",
): Promise<MediaRef> {
  const source = new TextEncoder().encode(serializeDrawDocumentJson(document));
  const preview = await drawDocumentPng(document);
  const [sourceUpload, previewUpload] = await Promise.all([
    uploadMediaBytes(sessionId, "drawing.draw.json", DRAW_MEDIA_MIME, source, label),
    uploadMediaBytes(sessionId, "drawing.png", "image/png", preview, label),
  ]);
  return {
    media_id: previewUpload.media_id,
    source_media_id: sourceUpload.media_id,
    kind: "drawing",
    mime: "image/png",
    label,
  };
}

export function drawDocumentFromBytes(bytes: Uint8Array): DrawDocument {
  return parseDrawDocumentJson(new TextDecoder().decode(bytes));
}
