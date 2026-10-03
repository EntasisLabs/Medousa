/** Complete, wrapped code snippets on PDF and Word export paper. */
export function buildCodeExportCss(monoFont: string): string {
  return `
  .vault-pdf-export-mount .markdown-code-block,
  .vault-pdf-export-mount pre,
  .vault-pdf-export-mount .markdown-pre {
    background: #f3f4f6 !important;
    border: 1px solid #d1d5db !important;
    border-radius: 6px !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount code,
  .vault-pdf-export-mount .markdown-code {
    background: #f3f4f6 !important;
    color: #111827 !important;
    font-family: ${monoFont} !important;
  }

  .vault-pdf-export-mount .markdown-pre {
    overflow: visible !important;
  }

  .vault-pdf-export-mount .markdown-code {
    white-space: pre-wrap !important;
    overflow-wrap: anywhere !important;
  }

  .vault-pdf-export-mount :not(pre) > code {
    padding: 0.1rem 0.35rem !important;
    border-radius: 4px !important;
  }
  `;
}
