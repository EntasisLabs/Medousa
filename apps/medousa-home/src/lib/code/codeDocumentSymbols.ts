import type { CodeDocumentSymbol } from "$lib/code/codingEngineClient";

export type CodeBreadcrumbSymbol = {
  name: string;
  line: number;
};

function symbolLine(symbol: CodeDocumentSymbol): number {
  return (
    (symbol.selectionRange?.start?.line ?? symbol.range?.start?.line ?? 0) + 1
  );
}

function symbolEndLine(symbol: CodeDocumentSymbol): number {
  return (symbol.range?.end?.line ?? symbol.range?.start?.line ?? 0) + 1;
}

/**
 * Walk nested document symbols and return the ancestor chain that contains
 * the given 1-based line (outermost → innermost).
 */
export function containingSymbolTrail(
  symbols: CodeDocumentSymbol[],
  line: number | null | undefined,
): CodeBreadcrumbSymbol[] {
  if (!line || line < 1 || symbols.length === 0) return [];
  const trail: CodeBreadcrumbSymbol[] = [];

  function walk(nodes: CodeDocumentSymbol[]): boolean {
    for (const symbol of nodes) {
      const start = symbolLine(symbol);
      const end = Math.max(start, symbolEndLine(symbol));
      if (line! < start || line! > end) continue;
      trail.push({ name: symbol.name, line: start });
      if (symbol.children?.length) walk(symbol.children);
      return true;
    }
    return false;
  }

  // Flat servers may omit ranges.end — fall back to best-effort nearest start.
  if (!walk(symbols)) {
    const flat: CodeDocumentSymbol[] = [];
    const flatten = (nodes: CodeDocumentSymbol[]) => {
      for (const node of nodes) {
        flat.push(node);
        if (node.children?.length) flatten(node.children);
      }
    };
    flatten(symbols);
    const containing = flat
      .filter((symbol) => line >= symbolLine(symbol))
      .sort((a, b) => symbolLine(b) - symbolLine(a))[0];
    if (containing) trail.push({ name: containing.name, line: symbolLine(containing) });
  }

  return trail;
}

export function pathBreadcrumbSegments(path: string): Array<{
  label: string;
  /** Directory prefix, or full path for the file segment. */
  path: string;
  isFile: boolean;
}> {
  const parts = path.split("/").filter(Boolean);
  if (parts.length === 0) return [];
  return parts.map((label, index) => {
    const segmentPath = parts.slice(0, index + 1).join("/");
    return {
      label,
      path: segmentPath,
      isFile: index === parts.length - 1,
    };
  });
}

export type CodeBreadcrumbSegment = {
  label: string;
  path: string;
  isFile: boolean;
  /** Collapsed middle marker — not navigable. */
  ellipsis?: boolean;
};

/**
 * Collapse deep paths to `root › … › parent › file` (head + last `tailCount`).
 */
export function collapsePathBreadcrumbs(
  segments: Array<{ label: string; path: string; isFile: boolean }>,
  options?: { headCount?: number; tailCount?: number },
): CodeBreadcrumbSegment[] {
  const headCount = options?.headCount ?? 1;
  const tailCount = options?.tailCount ?? 2;
  if (segments.length <= headCount + tailCount) {
    return segments.map((segment) => ({ ...segment }));
  }
  const head = segments.slice(0, headCount);
  const tail = segments.slice(-tailCount);
  return [
    ...head.map((segment) => ({ ...segment })),
    { label: "…", path: "", isFile: false, ellipsis: true },
    ...tail.map((segment) => ({ ...segment })),
  ];
}

/** Prefer the innermost symbol(s) for the trail — avoid stacking the whole chain. */
export function collapseSymbolTrail(
  symbols: CodeBreadcrumbSymbol[],
  maxLeaf = 1,
): CodeBreadcrumbSymbol[] {
  if (symbols.length <= maxLeaf) return symbols;
  return symbols.slice(-maxLeaf);
}
