import { invoke } from "@tauri-apps/api/core";
import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
import { destroyLiquidEmbeds } from "$lib/markdown/hydrateLiquidEmbeds";
import { renderMarkdownPreview } from "$lib/markdown";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { isTauri } from "$lib/window";

const PDF_EXPORT_CSS = `
  .vault-pdf-export-mount,
  .vault-pdf-export-mount * {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }

  .vault-pdf-export-mount {
    background: #ffffff !important;
    color: #111827 !important;
    font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif !important;
    font-size: 14px !important;
    line-height: 1.65 !important;
  }

  .vault-pdf-export-mount h1,
  .vault-pdf-export-mount h2,
  .vault-pdf-export-mount h3,
  .vault-pdf-export-mount h4,
  .vault-pdf-export-mount h5,
  .vault-pdf-export-mount h6 {
    color: #111827 !important;
    font-weight: 600 !important;
  }

  .vault-pdf-export-mount h1 { font-size: 1.5rem !important; margin: 0 0 1rem !important; }
  .vault-pdf-export-mount h2 { font-size: 1.25rem !important; margin: 1.25rem 0 0.5rem !important; }
  .vault-pdf-export-mount h3 { font-size: 1.1rem !important; margin: 1rem 0 0.5rem !important; }

  .vault-pdf-export-mount p,
  .vault-pdf-export-mount li,
  .vault-pdf-export-mount td,
  .vault-pdf-export-mount th,
  .vault-pdf-export-mount blockquote,
  .vault-pdf-export-mount span,
  .vault-pdf-export-mount strong,
  .vault-pdf-export-mount em,
  .vault-pdf-export-mount div {
    color: #111827 !important;
  }

  .vault-pdf-export-mount a {
    color: #2563eb !important;
    text-decoration: underline !important;
  }

  .vault-pdf-export-mount blockquote {
    border-left: 3px solid #d1d5db !important;
    padding-left: 12px !important;
    color: #374151 !important;
  }

  .vault-pdf-export-mount ul { list-style: disc !important; padding-left: 1.25rem !important; }
  .vault-pdf-export-mount ol { list-style: decimal !important; padding-left: 1.25rem !important; }

  .vault-pdf-export-mount table {
    width: 100% !important;
    border-collapse: collapse !important;
    margin: 12px 0 !important;
  }

  .vault-pdf-export-mount th,
  .vault-pdf-export-mount td {
    border: 1px solid #d1d5db !important;
    padding: 6px 10px !important;
  }

  .vault-pdf-export-mount th {
    background: #f3f4f6 !important;
    font-weight: 600 !important;
  }

  .vault-pdf-export-mount .markdown-code-block,
  .vault-pdf-export-mount pre,
  .vault-pdf-export-mount .markdown-pre {
    background: #f3f4f6 !important;
    border: 1px solid #d1d5db !important;
    border-radius: 6px !important;
  }

  .vault-pdf-export-mount code,
  .vault-pdf-export-mount .markdown-code {
    background: #f3f4f6 !important;
    color: #111827 !important;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace !important;
  }

  .vault-pdf-export-mount :not(pre) > code {
    padding: 0.1rem 0.35rem !important;
    border-radius: 4px !important;
  }

  .vault-pdf-export-mount .markdown-code-copy,
  .vault-pdf-export-mount .markdown-wikilink {
    display: none !important;
  }

  .vault-pdf-export-mount mark.markdown-highlight {
    background: #fef08a !important;
    color: #422006 !important;
  }

  .vault-pdf-export-mount .markdown-callout {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    border-radius: 6px !important;
    padding: 12px !important;
    margin: 12px 0 !important;
  }

  .vault-pdf-export-mount pre.mermaid {
    background: #f9fafb !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-md-embed {
    margin: 1rem 0 !important;
    break-inside: avoid;
  }

  .vault-pdf-export-mount .liquid-chart {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    color: #111827 !important;
    border-radius: 8px !important;
    padding: 12px !important;
    box-shadow: none !important;
  }

  .vault-pdf-export-mount .liquid-chart-tooltip {
    display: none !important;
  }

  .vault-pdf-export-mount .liquid-chart-title,
  .vault-pdf-export-mount .liquid-chart-center-value,
  .vault-pdf-export-mount .liquid-chart-value-label,
  .vault-pdf-export-mount .liquid-chart-pie-label,
  .vault-pdf-export-mount .liquid-chart-axis,
  .vault-pdf-export-mount .liquid-chart-radar-label,
  .vault-pdf-export-mount .liquid-chart-legend-label {
    color: #111827 !important;
    fill: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-chart-description,
  .vault-pdf-export-mount .liquid-chart-caption,
  .vault-pdf-export-mount .liquid-chart-center-label {
    color: #4b5563 !important;
    fill: #4b5563 !important;
  }

  .vault-pdf-export-mount .liquid-chart-mount {
    animation: none !important;
  }
`;

function slugifyFilename(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "note";
}

async function waitForPaint(): Promise<void> {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

/** Extra settle time for LayerCake / chart layout before capture. */
async function waitForLiquidLayout(): Promise<void> {
  await waitForPaint();
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 80);
  });
}

function buildPdfExportDom(title: string, html: string): {
  shell: HTMLElement;
  mount: HTMLElement;
  bodyEl: HTMLElement;
} {
  const shell = document.createElement("div");
  shell.className = "vault-pdf-export-shell";
  shell.style.cssText =
    "position:fixed;inset:0;z-index:2147483646;background:#ffffff;overflow:auto;pointer-events:none;";

  const mount = document.createElement("div");
  mount.className = "vault-pdf-export-mount";
  mount.style.cssText = "width:720px;max-width:720px;margin:0 auto;padding:48px 40px 64px;";

  const styleEl = document.createElement("style");
  styleEl.textContent = PDF_EXPORT_CSS;

  const titleEl = document.createElement("h1");
  titleEl.textContent = title;

  const bodyEl = document.createElement("div");
  bodyEl.className = "vault-pdf-export-body markdown-content";
  bodyEl.innerHTML = html;

  mount.append(styleEl, titleEl, bodyEl);
  shell.appendChild(mount);

  return { shell, mount, bodyEl };
}

export async function exportVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<void> {
  const body = stripFrontmatter(options.content).content;
  const html = renderMarkdownPreview(body, options.labelByPath);
  if (!html.trim()) {
    throw new Error("Nothing to export — note preview is empty.");
  }

  const filename = `${slugifyFilename(options.title)}.pdf`;
  const { shell, mount, bodyEl } = buildPdfExportDom(options.title, html);
  document.body.appendChild(shell);

  try {
    await hydrateMarkdownContainer(bodyEl, {
      liquidContext: {
        titleByPath: options.labelByPath,
        openLinksInWeb: false,
      },
      code: true,
      mermaid: true,
      liquid: true,
      animate: false,
    });
    await waitForLiquidLayout();

    const html2pdf = (await import("html2pdf.js")).default;
    const worker = html2pdf().set({
      margin: [0.55, 0.6, 0.55, 0.6],
      filename,
      image: { type: "jpeg", quality: 0.96 },
      html2canvas: {
        scale: 2,
        useCORS: true,
        backgroundColor: "#ffffff",
        scrollX: 0,
        scrollY: -window.scrollY,
        windowWidth: mount.scrollWidth,
        logging: false,
      },
      jsPDF: { unit: "in", format: "letter", orientation: "portrait" },
      pagebreak: { mode: ["css", "legacy"] },
    }).from(mount);

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
    destroyLiquidEmbeds(bodyEl);
    shell.remove();
  }
}

export async function downloadVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<void> {
  await exportVaultNotePdf(options);
}
