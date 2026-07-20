/**
 * Vault note → PDF via shared export prep + html2pdf.js.
 */

import {
  normalizeVaultExportOptions,
  saveExportBlob,
  vaultExportFilename,
  exportMarginInches,
  type VaultExportOptions,
} from "./vaultExportOptions";
import {
  prepareVaultExportMount,
  sanitizeUnsupportedCssColors,
  scrubUnsupportedColorFunctionsInClone,
} from "./vaultExportPrep";

export function vaultPdfFilename(title: string): string {
  return vaultExportFilename(title, "pdf");
}

export async function saveVaultNotePdfBlob(
  blob: Blob,
  filename: string,
): Promise<boolean> {
  return saveExportBlob(blob, filename, "pdf");
}

/** Hydrate note markdown → PDF blob (same bytes Save would write). */
export async function renderVaultNotePdfBlob(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
  notePath?: string | null;
  exportOptions?: Partial<VaultExportOptions> | null;
}): Promise<Blob> {
  const exportOptions = normalizeVaultExportOptions(options.exportOptions);
  const prepared = await prepareVaultExportMount({
    title: options.title,
    content: options.content,
    labelByPath: options.labelByPath,
    notePath: options.notePath,
    options: exportOptions,
  });

  const filename = vaultPdfFilename(options.title);
  const { mount, dispose, options: preparedOptions } = prepared;
  const isResume = mount.dataset.noteKind === "resume";

  try {
    const html2pdf = (await import("html2pdf.js")).default;
    const margins = exportMarginInches(preparedOptions.margins);
    const worker = html2pdf()
      .set({
        margin: margins,
        filename,
        image: { type: "jpeg", quality: 0.96 },
        html2canvas: {
          scale: 2,
          useCORS: true,
          backgroundColor: "#ffffff",
          scrollX: 0,
          scrollY: 0,
          windowWidth: mount.scrollWidth,
          logging: false,
          onclone: (clonedDoc: Document) => {
            scrubUnsupportedColorFunctionsInClone(clonedDoc);
            const clonedMount = clonedDoc.querySelector<HTMLElement>(
              ".vault-pdf-export-mount",
            );
            if (clonedMount) sanitizeUnsupportedCssColors(clonedMount);
          },
        },
        jsPDF: {
          unit: "in",
          format: preparedOptions.pageSize,
          orientation: preparedOptions.orientation,
        },
        // Always avoid splitting glued sections / compare (orphans + cropped slivers).
        // keepTogether adds smaller unit avoids on top.
        // Resumes: avoid job wrappers, not bare h3 (bare h3 orphans the list).
        pagebreak: {
          mode: ["css"],
          avoid: [
            // Always: never split mid-row / orphan thead; keep glued sections.
            "tr",
            "thead",
            ".vault-export-section",
            ".vault-export-keep",
            ".vault-export-label-group",
            ".vault-export-job",
            ".liquid-compare",
            '.liquid-md-embed[data-liquid-embed="compare"]',
            "h2",
            ...(isResume ? [] : ["h3"]),
            "h4",
            ...(preparedOptions.keepTogether
              ? [
                  "img",
                  ".liquid-callout",
                  ".liquid-compare-card",
                  ".liquid-compare-faceoff",
                  ".liquid-carousel-item",
                  ".liquid-brief",
                  ".liquid-tabs",
                  ".markdown-callout",
                  ".markdown-code-block",
                  "pre",
                ]
              : []),
          ],
        },
      })
      .from(mount);

    return (await worker.outputPdf("blob")) as Blob;
  } finally {
    dispose();
  }
}

/** One-shot render + save (no preview). Prefer the preview modal in UI. */
export async function exportVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
  notePath?: string | null;
  exportOptions?: Partial<VaultExportOptions> | null;
}): Promise<void> {
  const blob = await renderVaultNotePdfBlob(options);
  await saveVaultNotePdfBlob(blob, vaultPdfFilename(options.title));
}

export async function downloadVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
  notePath?: string | null;
  exportOptions?: Partial<VaultExportOptions> | null;
}): Promise<void> {
  await exportVaultNotePdf(options);
}
