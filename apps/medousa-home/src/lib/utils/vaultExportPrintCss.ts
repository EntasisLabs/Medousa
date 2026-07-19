/**
 * Print-paper CSS for vault export mounts (PDF + Word snapshot prep).
 * Hex/rgb only — html2canvas rejects color-mix().
 */

import {
  exportFontStack,
  exportMonoFontStack,
  type VaultExportOptions,
} from "./vaultExportOptions";

/** Build parameterized export stylesheet for `.vault-pdf-export-mount`. */
export function buildExportPrintCss(options: VaultExportOptions): string {
  const font = exportFontStack(options.fontFamily);
  const mono = exportMonoFontStack();
  const base = options.baseFontPx;
  const h1 = (base * 1.5).toFixed(2);
  const h2 = (base * 1.25).toFixed(2);
  const h3 = (base * 1.1).toFixed(2);
  const keep = options.keepTogether;
  const breakH2 = options.breakBeforeH2
    ? `
  .vault-pdf-export-mount h2 {
    break-before: page !important;
    page-break-before: always !important;
  }
  .vault-pdf-export-mount h1 + h2 {
    break-before: auto !important;
    page-break-before: auto !important;
  }`
    : "";

  // Small units only — never blanket whole `table` (must span pages at row bounds).
  const avoid = keep
    ? `
  .vault-pdf-export-mount pre,
  .vault-pdf-export-mount .markdown-code-block,
  .vault-pdf-export-mount .liquid-callout,
  .vault-pdf-export-mount .liquid-compare-card,
  .vault-pdf-export-mount .liquid-compare-faceoff,
  .vault-pdf-export-mount .liquid-carousel-item,
  .vault-pdf-export-mount .liquid-brief-section,
  .vault-pdf-export-mount .liquid-tabs-panel--export,
  .vault-pdf-export-mount blockquote,
  .vault-pdf-export-mount .markdown-callout,
  .vault-pdf-export-mount details {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }`
    : "";

  // Always: tall organisms may split; glue headers; tables split on rows only.
  const pageFlow = `
  .vault-pdf-export-mount .liquid-md-embed,
  .vault-pdf-export-mount .liquid-report,
  .vault-pdf-export-mount .liquid-compare,
  .vault-pdf-export-mount .liquid-chart,
  .vault-pdf-export-mount .liquid-tabs,
  .vault-pdf-export-mount .liquid-accordion,
  .vault-pdf-export-mount .liquid-carousel,
  .vault-pdf-export-mount .liquid-brief,
  .vault-pdf-export-mount .vault-export-section,
  .vault-pdf-export-mount table:not(.liquid-compare-table) {
    break-inside: auto !important;
    page-break-inside: auto !important;
  }

  .vault-pdf-export-mount tr {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }

  .vault-pdf-export-mount thead {
    display: table-header-group !important;
    break-inside: avoid !important;
    page-break-inside: avoid !important;
    break-after: avoid !important;
    page-break-after: avoid !important;
  }

  .vault-pdf-export-mount thead tr {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
    break-after: avoid !important;
    page-break-after: avoid !important;
  }

  /* Keep header with the first body row so we never end a page on thead alone. */
  .vault-pdf-export-mount tbody tr:first-child {
    break-before: avoid !important;
    page-break-before: avoid !important;
  }

  .vault-pdf-export-mount tfoot {
    display: table-footer-group !important;
  }

  .vault-pdf-export-mount .vault-export-keep,
  .vault-pdf-export-mount .vault-export-label-group {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }

  .vault-pdf-export-mount .vault-export-allow-break {
    break-inside: auto !important;
    page-break-inside: auto !important;
  }

  .vault-pdf-export-mount h2,
  .vault-pdf-export-mount h3,
  .vault-pdf-export-mount h4,
  .vault-pdf-export-mount .vault-export-section > h2,
  .vault-pdf-export-mount .vault-export-section > h3 {
    break-after: avoid !important;
    page-break-after: avoid !important;
  }

  .vault-pdf-export-mount .vault-export-section > .liquid-md-embed {
    break-before: avoid !important;
    page-break-before: avoid !important;
  }

  /* Word snapshots bake section headings into the PNG — keep spacing tight. */
  .vault-pdf-export-mount .vault-export-section > h2,
  .vault-pdf-export-mount .vault-export-section > h3 {
    margin-top: 0 !important;
    margin-bottom: 0.45rem !important;
  }

  .vault-pdf-export-mount .liquid-compare-header,
  .vault-pdf-export-mount .liquid-report-header,
  .vault-pdf-export-mount .liquid-tabs-header,
  .vault-pdf-export-mount .liquid-accordion-header,
  .vault-pdf-export-mount .liquid-brief-header {
    break-after: avoid !important;
    page-break-after: avoid !important;
  }

  /* Prefer clean splits between tab/brief sections, not through chrome. */
  .vault-pdf-export-mount .liquid-brief-section,
  .vault-pdf-export-mount .liquid-tabs-panel--export {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }`;

  return `
  .vault-pdf-export-mount,
  .vault-pdf-export-mount * {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }

  .vault-pdf-export-mount {
    background: #ffffff !important;
    color: #111827 !important;
    font-family: ${font} !important;
    font-size: ${base}px !important;
    line-height: 1.65 !important;
  }

  .vault-pdf-export-mount[data-export-paper="1"] {
    /* marker for prep / tests */
  }

  .vault-pdf-export-mount h1,
  .vault-pdf-export-mount h2,
  .vault-pdf-export-mount h3,
  .vault-pdf-export-mount h4,
  .vault-pdf-export-mount h5,
  .vault-pdf-export-mount h6 {
    color: #111827 !important;
    font-weight: 600 !important;
    font-family: inherit !important;
  }

  .vault-pdf-export-mount h1 {
    font-size: ${h1}px !important;
    margin: 0 0 1rem !important;
    break-before: auto !important;
  }

  .vault-pdf-export-mount > h1:first-child {
    break-before: auto !important;
    page-break-before: auto !important;
  }

  .vault-pdf-export-mount h2 {
    font-size: ${h2}px !important;
    margin: 1.25rem 0 0.5rem !important;
  }

  .vault-pdf-export-mount h3 {
    font-size: ${h3}px !important;
    margin: 1rem 0 0.5rem !important;
  }

  ${breakH2}

  .vault-pdf-export-mount .vault-export-page-break {
    break-before: page !important;
    page-break-before: always !important;
    height: 0 !important;
    margin: 0 !important;
    border: 0 !important;
    visibility: hidden !important;
  }

  /* Prose ink — descendants (ul > li), not only body > li which never matches. */
  .vault-pdf-export-mount .vault-pdf-export-body,
  .vault-pdf-export-mount .vault-pdf-export-body p,
  .vault-pdf-export-mount .vault-pdf-export-body li,
  .vault-pdf-export-mount .vault-pdf-export-body ul,
  .vault-pdf-export-mount .vault-pdf-export-body ol,
  .vault-pdf-export-mount .vault-pdf-export-body em,
  .vault-pdf-export-mount .vault-pdf-export-body strong,
  .vault-pdf-export-mount .vault-pdf-export-body li > *,
  .vault-pdf-export-mount .markdown-content p,
  .vault-pdf-export-mount .markdown-content li,
  .vault-pdf-export-mount .markdown-content ul,
  .vault-pdf-export-mount .markdown-content ol,
  .vault-pdf-export-mount .markdown-content em,
  .vault-pdf-export-mount .markdown-content strong,
  .vault-pdf-export-mount .markdown-content li > *,
  .vault-pdf-export-mount .markdown-table-scroll td,
  .vault-pdf-export-mount .markdown-table-scroll th {
    color: #111827 !important;
  }

  .vault-pdf-export-mount a,
  .vault-pdf-export-mount .markdown-wikilink {
    color: #2563eb !important;
    text-decoration: underline !important;
    background: transparent !important;
    display: inline !important;
    border: 0 !important;
    padding: 0 !important;
    cursor: default !important;
  }

  .vault-pdf-export-mount blockquote {
    border-left: 3px solid #d1d5db !important;
    padding-left: 12px !important;
    color: #374151 !important;
    background: transparent !important;
  }

  .vault-pdf-export-mount ul { list-style: disc !important; padding-left: 1.25rem !important; }
  .vault-pdf-export-mount ol { list-style: decimal !important; padding-left: 1.25rem !important; }

  .vault-pdf-export-mount .markdown-table-scroll {
    overflow: visible !important;
    width: 100% !important;
    max-width: 100% !important;
    min-width: 0 !important;
    margin: 0.75rem 0 !important;
    margin-left: 0 !important;
    margin-right: 0 !important;
    padding: 0 !important;
    border: 0 !important;
    box-sizing: border-box !important;
    transform: none !important;
  }

  .vault-pdf-export-mount .markdown-table-scroll table,
  .vault-pdf-export-mount table:not(.liquid-compare-table) {
    width: 100% !important;
    max-width: 100% !important;
    min-width: 0 !important;
    border-collapse: collapse !important;
    margin: 0.75rem 0 !important;
    margin-left: 0 !important;
    margin-right: 0 !important;
    table-layout: auto !important;
    box-sizing: border-box !important;
  }

  /* Align heading left edge with table outer border (not cell padding). */
  .vault-pdf-export-mount .vault-pdf-export-body > h1,
  .vault-pdf-export-mount .vault-pdf-export-body > h2,
  .vault-pdf-export-mount .vault-pdf-export-body > h3,
  .vault-pdf-export-mount .vault-pdf-export-body > h4,
  .vault-pdf-export-mount .vault-pdf-export-body > p,
  .vault-pdf-export-mount .vault-export-label-group > p {
    margin-left: 0 !important;
    padding-left: 0 !important;
  }

  .vault-pdf-export-mount .markdown-table-scroll th,
  .vault-pdf-export-mount .markdown-table-scroll td,
  .vault-pdf-export-mount table:not(.liquid-compare-table) th,
  .vault-pdf-export-mount table:not(.liquid-compare-table) td {
    border: 1px solid #d1d5db !important;
    padding: 6px 8px !important;
    word-break: break-word !important;
    box-sizing: border-box !important;
  }

  .vault-pdf-export-mount .markdown-table-scroll th {
    background: #f3f4f6 !important;
    font-weight: 600 !important;
  }

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
    font-family: ${mono} !important;
  }

  .vault-pdf-export-mount :not(pre) > code {
    padding: 0.1rem 0.35rem !important;
    border-radius: 4px !important;
  }

  .vault-pdf-export-mount .markdown-code-copy,
  .vault-pdf-export-mount .liquid-chart-toolbar,
  .vault-pdf-export-mount .liquid-chart-configure,
  .vault-pdf-export-mount .liquid-chart-tooltip,
  .vault-pdf-export-mount .vault-live-quiet-chrome {
    display: none !important;
  }

  .vault-pdf-export-mount mark.markdown-highlight {
    background: #fef08a !important;
    color: #422006 !important;
  }

  .vault-pdf-export-mount .markdown-callout {
    border: 1px solid #9ca3af !important;
    background: #f3f4f6 !important;
    border-radius: 6px !important;
    padding: 12px 14px !important;
    margin: 12px 0 !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .markdown-callout,
  .vault-pdf-export-mount .markdown-callout p,
  .vault-pdf-export-mount .markdown-callout li,
  .vault-pdf-export-mount .markdown-callout em,
  .vault-pdf-export-mount .markdown-callout strong {
    color: #111827 !important;
  }

  /* —— Liquid callout (paper) —— */
  .vault-pdf-export-mount .liquid-callout {
    margin: 12px 0 !important;
    padding: 14px 16px !important;
    border-radius: 10px !important;
    border: 1px solid #9ca3af !important;
    background: #f3f4f6 !important;
    background-image: none !important;
    box-shadow: none !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-callout-title,
  .vault-pdf-export-mount .liquid-callout-body,
  .vault-pdf-export-mount .liquid-callout-body p,
  .vault-pdf-export-mount .vault-live-callout__title,
  .vault-pdf-export-mount .vault-live-callout__body {
    color: #111827 !important;
  }

  /* —— Liquid accordion / FAQ (paper) —— */
  .vault-pdf-export-mount .liquid-accordion {
    margin: 12px 0 !important;
    padding: 12px 14px !important;
    border-radius: 10px !important;
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    background-image: none !important;
    box-shadow: none !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-accordion-title {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-accordion-subtitle {
    color: #374151 !important;
  }

  .vault-pdf-export-mount .liquid-accordion-item {
    border: 1px solid #e5e7eb !important;
    background: #ffffff !important;
    border-radius: 8px !important;
    margin: 0 0 8px !important;
  }

  .vault-pdf-export-mount .liquid-accordion-trigger {
    cursor: default !important;
    pointer-events: none !important;
    background: #ffffff !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-accordion-label,
  .vault-pdf-export-mount .liquid-accordion-panel,
  .vault-pdf-export-mount .liquid-accordion-panel p {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-accordion-chevron {
    opacity: 0.35 !important;
    color: #6b7280 !important;
  }

  .vault-pdf-export-mount .liquid-accordion-panel {
    display: block !important;
    background: #ffffff !important;
    padding: 0 12px 12px !important;
  }

  /* —— Brief / cite (paper) —— */
  .vault-pdf-export-mount .liquid-brief,
  .vault-pdf-export-mount .liquid-cite {
    margin: 12px 0 !important;
    padding: 14px 16px !important;
    border-radius: 10px !important;
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    background-image: none !important;
    box-shadow: none !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-brief-title,
  .vault-pdf-export-mount .liquid-brief-heading,
  .vault-pdf-export-mount .liquid-brief-body,
  .vault-pdf-export-mount .liquid-brief-body p {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-brief-subtitle,
  .vault-pdf-export-mount .liquid-brief-tone {
    color: #374151 !important;
  }

  .vault-pdf-export-mount .liquid-brief-header {
    border-bottom: 1px solid #e5e7eb !important;
  }

  .vault-pdf-export-mount .liquid-brief-section {
    break-inside: avoid !important;
  }

  /* —— Liquid field cards (paper) —— */
  .vault-pdf-export-mount .liquid-card,
  .vault-pdf-export-mount .vault-live-card {
    margin: 10px 0 !important;
    padding: 12px 14px !important;
    border-radius: 10px !important;
    border: 1px solid #d1d5db !important;
    background: #ffffff !important;
    background-image: none !important;
    box-shadow: none !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-card-main {
    cursor: default !important;
    pointer-events: none !important;
    background: transparent !important;
  }

  .vault-pdf-export-mount .liquid-card-title,
  .vault-pdf-export-mount .liquid-card-body,
  .vault-pdf-export-mount .liquid-card-body p {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-card-subtitle,
  .vault-pdf-export-mount .liquid-card-meta {
    color: #374151 !important;
  }

  /* —— Carousel (export: wrap grid, never clip) —— */
  .vault-pdf-export-mount .liquid-carousel,
  .vault-pdf-export-mount .liquid-carousel--export {
    display: flex !important;
    flex-wrap: wrap !important;
    gap: 0.65rem !important;
    overflow: visible !important;
    scroll-snap-type: none !important;
    mask-image: none !important;
    -webkit-mask-image: none !important;
    padding: 0.1rem 0 !important;
  }

  .vault-pdf-export-mount .liquid-carousel-item {
    flex: 1 1 calc(50% - 0.4rem) !important;
    width: auto !important;
    min-width: min(14rem, 100%) !important;
    max-width: 100% !important;
    scroll-snap-align: none !important;
  }

  /* —— Tabs (export: all panels stacked; hide interactive strip) —— */
  .vault-pdf-export-mount .liquid-tabs {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    background-image: none !important;
    box-shadow: none !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-tabs-list {
    display: none !important;
  }

  .vault-pdf-export-mount .liquid-tabs-tab {
    pointer-events: none !important;
    cursor: default !important;
  }

  .vault-pdf-export-mount .liquid-tabs-title,
  .vault-pdf-export-mount .liquid-tabs-panel,
  .vault-pdf-export-mount .liquid-tabs-panel p {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-tabs-export-label {
    color: #4b5563 !important;
  }

  .vault-pdf-export-mount .liquid-tabs-panel--export {
    margin-top: 0.65rem !important;
    padding: 0.55rem 0.65rem !important;
    border: 1px solid #e5e7eb !important;
    border-radius: 8px !important;
    background: #ffffff !important;
  }

  .vault-pdf-export-mount .liquid-mini-kanban {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    color: #111827 !important;
    padding: 12px !important;
    border-radius: 8px !important;
  }

  .vault-pdf-export-mount .liquid-mini-kanban__column-title,
  .vault-pdf-export-mount .liquid-mini-kanban__card,
  .vault-pdf-export-mount .liquid-mini-kanban__label {
    color: #111827 !important;
  }

  .vault-pdf-export-mount details {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    border-radius: 8px !important;
    padding: 10px 12px !important;
    margin: 10px 0 !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount details summary {
    font-weight: 600 !important;
    color: #111827 !important;
    cursor: default !important;
  }

  .vault-pdf-export-mount pre.mermaid {
    background: #f9fafb !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-md-embed {
    margin: 1rem 0 !important;
    min-width: 0 !important;
    width: 100% !important;
  }

  /* —— Chart / report (paper) —— */
  .vault-pdf-export-mount .liquid-chart {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    color: #111827 !important;
    border-radius: 8px !important;
    padding: 12px !important;
    box-shadow: none !important;
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

  .vault-pdf-export-mount .liquid-report {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    box-shadow: none !important;
    color: #111827 !important;
    border-radius: 8px !important;
    padding: 14px 16px 16px !important;
  }

  .vault-pdf-export-mount .liquid-report-header {
    border-bottom: 1px solid #e5e7eb !important;
  }

  .vault-pdf-export-mount .liquid-report-title {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-report-subtitle,
  .vault-pdf-export-mount .liquid-report-body,
  .vault-pdf-export-mount .liquid-report-body .markdown-content,
  .vault-pdf-export-mount .liquid-report-body .markdown-content p {
    color: #374151 !important;
  }

  .vault-pdf-export-mount .liquid-report-body .markdown-content h1,
  .vault-pdf-export-mount .liquid-report-body .markdown-content h2,
  .vault-pdf-export-mount .liquid-report-body .markdown-content h3,
  .vault-pdf-export-mount .liquid-report-body .markdown-content h4 {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-chart-heatmap-wrap {
    background: transparent !important;
  }

  .vault-pdf-export-mount .liquid-chart-heatmap-col-label,
  .vault-pdf-export-mount .liquid-chart-heatmap-row-label {
    color: #4b5563 !important;
  }

  .vault-pdf-export-mount .liquid-chart-heatmap-cell {
    border: none !important;
    box-shadow: none !important;
  }

  .vault-pdf-export-mount .liquid-chart-grid {
    stroke: #e5e7eb !important;
  }

  .vault-pdf-export-mount .liquid-chart-axis-right {
    fill: #7c3aed !important;
    color: #7c3aed !important;
  }

  /* —— Compare matrix + faceoff (critical print-paper pack) —— */
  .vault-pdf-export-mount .liquid-compare {
    margin: 0 !important;
    padding: 14px 16px 16px !important;
    border-radius: 10px !important;
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    box-shadow: none !important;
    color: #111827 !important;
    min-width: 0 !important;
    width: 100% !important;
  }

  .vault-pdf-export-mount .liquid-compare-title {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-compare-subtitle {
    color: #4b5563 !important;
  }

  .vault-pdf-export-mount .liquid-compare-rec-banner {
    color: #5b21b6 !important;
    background: #ede9fe !important;
    border: 1px solid #c4b5fd !important;
  }

  .vault-pdf-export-mount .liquid-compare-rec-banner strong {
    color: #4c1d95 !important;
  }

  .vault-pdf-export-mount .liquid-compare-scroll {
    overflow: visible !important;
    overflow-x: visible !important;
    max-width: 100% !important;
    width: 100% !important;
    border: 1px solid #e5e7eb !important;
    background: #ffffff !important;
    border-radius: 8px !important;
    backdrop-filter: none !important;
  }

  .vault-pdf-export-mount .liquid-compare-table {
    width: 100% !important;
    min-width: 0 !important;
    max-width: 100% !important;
    table-layout: fixed !important;
    font-size: 0.65rem !important;
  }

  .vault-pdf-export-mount .liquid-compare-corner,
  .vault-pdf-export-mount .liquid-compare-axis {
    position: static !important;
    left: auto !important;
    z-index: auto !important;
    backdrop-filter: none !important;
    background: #f3f4f6 !important;
    color: #374151 !important;
    white-space: normal !important;
    word-break: break-word !important;
    overflow-wrap: anywhere !important;
    min-width: 0 !important;
    max-width: none !important;
    width: 14% !important;
    padding: 4px 3px !important;
  }

  .vault-pdf-export-mount .liquid-compare-entity {
    background: #f9fafb !important;
    min-width: 0 !important;
    max-width: none !important;
    width: auto !important;
    vertical-align: top !important;
    padding: 4px 3px !important;
  }

  .vault-pdf-export-mount .liquid-compare-entity-rec {
    background: #f5f3ff !important;
    box-shadow: inset 0 -2px 0 #8b5cf6 !important;
  }

  .vault-pdf-export-mount .liquid-compare-entity-btn {
    cursor: default !important;
    pointer-events: none !important;
    background: transparent !important;
    padding: 0.15rem 0.1rem !important;
  }

  .vault-pdf-export-mount .liquid-compare-entity-label {
    color: #111827 !important;
    font-size: 0.65rem !important;
    line-height: 1.25 !important;
    white-space: normal !important;
    word-break: break-word !important;
  }

  .vault-pdf-export-mount .liquid-compare-rec-whisper {
    color: #6d28d9 !important;
  }

  .vault-pdf-export-mount .liquid-compare-cell {
    color: #111827 !important;
    background: #ffffff !important;
    padding: 4px 3px !important;
    font-size: 0.65rem !important;
    line-height: 1.3 !important;
    word-break: break-word !important;
    overflow-wrap: anywhere !important;
  }

  .vault-pdf-export-mount tbody tr:nth-child(even) .liquid-compare-cell:not(.liquid-compare-cell-rec) {
    background: #f9fafb !important;
  }

  .vault-pdf-export-mount .liquid-compare-cell-rec {
    background: #f5f3ff !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount tbody tr:nth-child(even) .liquid-compare-cell-rec {
    background: #ede9fe !important;
  }

  .vault-pdf-export-mount .liquid-compare-faceoff {
    display: grid !important;
    grid-template-columns: repeat(2, minmax(0, 1fr)) !important;
    gap: 0.75rem !important;
  }

  .vault-pdf-export-mount .liquid-compare-card {
    border: 1px solid #d1d5db !important;
    background: #ffffff !important;
    box-shadow: none !important;
    color: #111827 !important;
    cursor: default !important;
    pointer-events: none !important;
  }

  .vault-pdf-export-mount .liquid-compare-card:hover {
    border-color: #d1d5db !important;
    background: #ffffff !important;
  }

  .vault-pdf-export-mount .liquid-compare-card--rec {
    border-color: #8b5cf6 !important;
    background: #faf5ff !important;
    box-shadow: inset 0 0 0 1px #c4b5fd !important;
  }

  .vault-pdf-export-mount .liquid-compare-card__label,
  .vault-pdf-export-mount .liquid-compare-card__value {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-compare-card__axis {
    color: #6b7280 !important;
  }

  .vault-pdf-export-mount .liquid-compare-card__badge,
  .vault-pdf-export-mount .liquid-compare-card__footer {
    color: #5b21b6 !important;
    background: #ede9fe !important;
  }

  .vault-pdf-export-mount .liquid-compare-card__point {
    background: #f3f4f6 !important;
    border: 1px solid #e5e7eb !important;
    color: #111827 !important;
  }

  /* Kanban light shell */
  .vault-pdf-export-mount .vault-live-kanban,
  .vault-pdf-export-mount [data-liquid-embed="kanban"] {
    background: #f9fafb !important;
    border: 1px solid #d1d5db !important;
    color: #111827 !important;
  }

  ${pageFlow}
  ${avoid}
`;
}
