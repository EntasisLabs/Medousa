import { invoke } from "@tauri-apps/api/core";
import { fetchArtifact } from "$lib/daemon";
import { shareText } from "$lib/share";
import {
  prepareArtifactHtml,
  type ArtifactEmbedMode,
} from "$lib/utils/artifactPrepareHtml";
import { copyTextToClipboard } from "$lib/utils/vaultClipboard";
import { isTauri } from "$lib/window";

const EXPORT_IFRAME_WIDTH_PX = 1280;

function slugifyFilename(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "presentation";
}

function isDarkTheme(): boolean {
  if (typeof document === "undefined") return true;
  return document.documentElement.classList.contains("dark");
}

async function waitForPaint(): Promise<void> {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

async function loadArtifact(sessionId: string, artifactId: string) {
  const response = await fetchArtifact(sessionId, artifactId);
  if (!response.mime.includes("html")) {
    throw new Error("Only HTML presentations can be exported.");
  }
  return response;
}

function artifactReference(sessionId: string, artifactId: string): string {
  return `medousa:artifact/${sessionId}/${artifactId}`;
}

export function artifactPathForCopy(
  sessionId: string,
  artifactId: string,
  payloadPath?: string | null,
): string {
  const trimmed = payloadPath?.trim();
  if (trimmed) return trimmed;
  return artifactReference(sessionId, artifactId);
}

export async function copyArtifactPath(
  sessionId: string,
  artifactId: string,
): Promise<boolean> {
  const response = await loadArtifact(sessionId, artifactId);
  return copyTextToClipboard(
    artifactPathForCopy(sessionId, artifactId, response.payload_path),
  );
}

export async function copyArtifactId(artifactId: string): Promise<boolean> {
  return copyTextToClipboard(artifactId);
}

export async function shareArtifact(
  sessionId: string,
  artifactId: string,
  label: string,
): Promise<"shared" | "copied" | "failed"> {
  const response = await loadArtifact(sessionId, artifactId);
  const path = artifactPathForCopy(sessionId, artifactId, response.payload_path);
  const text = [
    label.trim(),
    "",
    `Artifact: ${artifactId}`,
    `Session: ${sessionId}`,
    `Path: ${path}`,
  ].join("\n");
  return shareText(label, text);
}

async function buildExportIframe(
  html: string,
): Promise<{ shell: HTMLDivElement; iframe: HTMLIFrameElement }> {
  const shell = document.createElement("div");
  shell.className = "artifact-export-shell";
  shell.style.cssText =
    "position:fixed;left:-10000px;top:0;width:1280px;overflow:hidden;opacity:0;pointer-events:none;z-index:-1;";

  const iframe = document.createElement("iframe");
  iframe.title = "Artifact export";
  iframe.setAttribute("sandbox", "allow-scripts allow-same-origin");
  iframe.srcdoc = html;
  iframe.style.width = `${EXPORT_IFRAME_WIDTH_PX}px`;
  iframe.style.height = "2400px";
  iframe.style.border = "0";

  shell.appendChild(iframe);
  document.body.appendChild(shell);

  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Export preview timed out.")), 15000);
    iframe.onload = () => {
      clearTimeout(timeout);
      resolve();
    };
  });
  await waitForPaint();
  await new Promise((resolve) => setTimeout(resolve, 120));

  return { shell, iframe };
}

function exportCaptureTarget(iframe: HTMLIFrameElement): HTMLElement {
  const doc = iframe.contentDocument;
  if (!doc?.body) {
    throw new Error("Could not read the presentation for export.");
  }
  return doc.documentElement;
}

async function saveBlobFile(
  blob: Blob,
  filename: string,
  filters: { name: string; extensions: string[] }[],
  title: string,
): Promise<boolean> {
  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: filename, filters, title });
    if (!path) return false;
    const bytes = new Uint8Array(await blob.arrayBuffer());
    await invoke("write_file_bytes", { path, bytes: Array.from(bytes) });
    return true;
  }

  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
  return true;
}

export async function exportArtifactHtml(
  sessionId: string,
  artifactId: string,
  label: string,
): Promise<boolean> {
  const response = await loadArtifact(sessionId, artifactId);
  const filename = `${slugifyFilename(label)}.html`;
  const blob = new Blob([response.body], { type: "text/html;charset=utf-8" });
  return saveBlobFile(blob, filename, [{ name: "HTML", extensions: ["html"] }], "Save presentation as HTML");
}

export async function exportArtifactPdf(
  sessionId: string,
  artifactId: string,
  label: string,
  mode: ArtifactEmbedMode = "panel",
): Promise<boolean> {
  const response = await loadArtifact(sessionId, artifactId);
  const html = prepareArtifactHtml(response.body, mode, isDarkTheme());
  const filename = `${slugifyFilename(label)}.pdf`;
  const { shell, iframe } = await buildExportIframe(html);

  try {
    const target = exportCaptureTarget(iframe);
    const html2pdf = (await import("html2pdf.js")).default;
    const worker = html2pdf().set({
      margin: [0.35, 0.4, 0.35, 0.4],
      filename,
      image: { type: "jpeg", quality: 0.96 },
      html2canvas: {
        scale: 2,
        useCORS: true,
        backgroundColor: "#ffffff",
        scrollX: 0,
        scrollY: 0,
        windowWidth: target.scrollWidth || EXPORT_IFRAME_WIDTH_PX,
        logging: false,
      },
      jsPDF: { unit: "in", format: "letter", orientation: "portrait" },
      pagebreak: { mode: ["css", "legacy"] },
    }).from(target);

    if (isTauri()) {
      const blob = (await worker.outputPdf("blob")) as Blob;
      return saveBlobFile(blob, filename, [{ name: "PDF", extensions: ["pdf"] }], "Save presentation as PDF");
    }

    await worker.save();
    return true;
  } finally {
    shell.remove();
  }
}

export async function exportArtifactPng(
  sessionId: string,
  artifactId: string,
  label: string,
  mode: ArtifactEmbedMode = "panel",
): Promise<boolean> {
  const response = await loadArtifact(sessionId, artifactId);
  const html = prepareArtifactHtml(response.body, mode, isDarkTheme());
  const filename = `${slugifyFilename(label)}.png`;
  const { shell, iframe } = await buildExportIframe(html);

  try {
    const target = exportCaptureTarget(iframe);
    const html2canvas = (await import("html2canvas")).default;
    const canvas = await html2canvas(target, {
      scale: 2,
      useCORS: true,
      backgroundColor: "#ffffff",
      scrollX: 0,
      scrollY: 0,
      windowWidth: target.scrollWidth || EXPORT_IFRAME_WIDTH_PX,
      logging: false,
    });

    const blob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (value) => {
          if (!value) {
            reject(new Error("Could not render PNG."));
            return;
          }
          resolve(value);
        },
        "image/png",
        1,
      );
    });

    return saveBlobFile(blob, filename, [{ name: "PNG", extensions: ["png"] }], "Save presentation as PNG");
  } finally {
    shell.remove();
  }
}
