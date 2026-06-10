import { invoke } from "@tauri-apps/api/core";
import { renderMarkdownPreview } from "$lib/markdown";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { isTauri } from "$lib/window";

function slugifyFilename(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "note";
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

export async function exportVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<void> {
  const body = stripFrontmatter(options.content).content;
  const html = renderMarkdownPreview(body, options.labelByPath);
  const filename = `${slugifyFilename(options.title)}.pdf`;

  const mount = document.createElement("div");
  mount.style.cssText =
    "position: fixed; left: -10000px; top: 0; width: 720px; background: white; color: #111827;";
  const titleEl = document.createElement("h1");
  titleEl.textContent = options.title;
  titleEl.style.cssText = "margin: 0 0 1rem; font-size: 1.5rem; font-weight: 600;";
  const bodyEl = document.createElement("div");
  bodyEl.className = "markdown-content vault-pdf-export-root";
  bodyEl.innerHTML = html;
  mount.appendChild(titleEl);
  mount.appendChild(bodyEl);
  document.body.appendChild(mount);

  try {
    const html2pdf = (await import("html2pdf.js")).default;
    const worker = html2pdf()
      .set({
        margin: [0.55, 0.6, 0.55, 0.6],
        filename,
        image: { type: "jpeg", quality: 0.96 },
        html2canvas: { scale: 2, useCORS: true, backgroundColor: "#ffffff" },
        jsPDF: { unit: "in", format: "letter", orientation: "portrait" },
        pagebreak: { mode: ["css", "legacy"] },
      })
      .from(mount);

    if (isTauri()) {
      const blob = (await worker.outputPdf("blob")) as Blob;
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        defaultPath: filename,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
        title: "Export note as PDF",
      });
      if (!path) return;
      const bytes = new Uint8Array(await blob.arrayBuffer());
      await invoke("write_file_bytes", { path, bytes: Array.from(bytes) });
      return;
    }

    await worker.save();
  } finally {
    mount.remove();
  }
}

export async function downloadVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<void> {
  await exportVaultNotePdf(options);
}
