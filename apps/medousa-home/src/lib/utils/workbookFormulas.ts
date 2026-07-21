/**
 * HyperFormula overlay for vault workbooks.
 * Computed values are never written back into markdown (0.4.0 overlay-only).
 */

import { HyperFormula, type RawCellContent } from "hyperformula";
import {
  a1ToRowCol,
  colIndexToLetters,
  parseSheetFormulas,
  parseWorkbookManifest,
  sheetStem,
  tableGridFromMarkdown,
  type WorkbookManifest,
} from "$lib/utils/workbook";

export type SheetSource = {
  /** Stem name matching manifest `sheets:` entry. */
  name: string;
  markdown: string;
};

export type OverlayCell = {
  addr: string;
  /** Display string for Live/Build. */
  display: string;
  /** True when this cell has a formula in frontmatter. */
  fromFormula: boolean;
  error?: string;
};

export type SheetOverlay = {
  name: string;
  /** A1 → overlay */
  cells: Record<string, OverlayCell>;
};

export type WorkbookOverlay = {
  title: string;
  sheets: SheetOverlay[];
};

function cellLiteral(value: string): RawCellContent {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  return trimmed;
}

/** Build a multi-sheet HyperFormula instance and return overlay values. */
export function evaluateWorkbookOverlay(
  manifestMarkdown: string,
  sheets: SheetSource[],
): WorkbookOverlay | null {
  const manifest = parseWorkbookManifest(manifestMarkdown);
  if (!manifest) return null;
  return evaluateWorkbookOverlayFromManifest(manifest, sheets);
}

export function evaluateWorkbookOverlayFromManifest(
  manifest: WorkbookManifest,
  sheets: SheetSource[],
): WorkbookOverlay {
  const byName = new Map(
    sheets.map((s) => [sheetStem(s.name), s] as const),
  );

  const sheetBodies: Array<{ name: string; grid: RawCellContent[][]; formulas: Record<string, string> }> =
    [];

  for (const name of manifest.sheets) {
    const stem = sheetStem(name);
    const source = byName.get(stem);
    const formulas = source ? parseSheetFormulas(source.markdown).formulas : {};
    const { headers, rows } = source
      ? tableGridFromMarkdown(source.markdown)
      : { headers: [], rows: [] };

    const width = Math.max(
      headers.length,
      ...rows.map((r) => r.length),
      ...Object.keys(formulas).map((addr) => {
        const rc = a1ToRowCol(addr);
        return rc ? rc.col + 1 : 0;
      }),
      1,
    );
    const height = Math.max(
      rows.length + (headers.length ? 1 : 0),
      ...Object.keys(formulas).map((addr) => {
        const rc = a1ToRowCol(addr);
        return rc ? rc.row + 1 : 0;
      }),
      1,
    );

    const grid: RawCellContent[][] = Array.from({ length: height }, () =>
      Array.from({ length: width }, () => null),
    );

    // Row 0 = headers (Excel-like); data starts at row 1 → A1 is header of col A.
    if (headers.length) {
      for (let c = 0; c < headers.length; c++) {
        grid[0]![c] = headers[c] ?? null;
      }
    }
    for (let r = 0; r < rows.length; r++) {
      const row = rows[r] ?? [];
      for (let c = 0; c < row.length; c++) {
        grid[r + 1]![c] = cellLiteral(row[c] ?? "");
      }
    }

    for (const [addr, formula] of Object.entries(formulas)) {
      const rc = a1ToRowCol(addr);
      if (!rc) continue;
      while (grid.length <= rc.row) {
        grid.push(Array.from({ length: width }, () => null));
      }
      while ((grid[rc.row]!.length) <= rc.col) {
        grid[rc.row]!.push(null);
      }
      grid[rc.row]![rc.col] = formula;
    }

    sheetBodies.push({ name: stem, grid, formulas });
  }

  const hf = HyperFormula.buildFromSheets(
    Object.fromEntries(sheetBodies.map((s) => [s.name, s.grid])),
    { licenseKey: "gpl-v3" },
  );

  const overlays: SheetOverlay[] = [];
  for (const body of sheetBodies) {
    const sheetId = hf.getSheetId(body.name);
    const cells: Record<string, OverlayCell> = {};
    if (sheetId === undefined) {
      overlays.push({ name: body.name, cells });
      continue;
    }
    const height = body.grid.length;
    const width = body.grid[0]?.length ?? 0;
    for (let r = 0; r < height; r++) {
      for (let c = 0; c < width; c++) {
        const addr = `${colIndexToLetters(c)}${r + 1}`;
        const fromFormula = Object.prototype.hasOwnProperty.call(body.formulas, addr);
        if (!fromFormula) continue;
        try {
          const value = hf.getCellValue({ sheet: sheetId, row: r, col: c });
          if (value && typeof value === "object" && "type" in value) {
            const err = value as { type: string; message?: string };
            cells[addr] = {
              addr,
              display: `#${err.type}`,
              fromFormula: true,
              error: err.message ?? err.type,
            };
          } else if (value === null || value === undefined) {
            cells[addr] = { addr, display: "", fromFormula: true };
          } else {
            cells[addr] = {
              addr,
              display: formatDisplay(value),
              fromFormula: true,
            };
          }
        } catch (e) {
          cells[addr] = {
            addr,
            display: "#ERROR",
            fromFormula: true,
            error: e instanceof Error ? e.message : String(e),
          };
        }
      }
    }
    overlays.push({ name: body.name, cells });
  }

  hf.destroy();
  return { title: manifest.title, sheets: overlays };
}

function formatDisplay(value: unknown): string {
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return String(value);
    if (Number.isInteger(value)) return String(value);
    return String(Math.round(value * 1e6) / 1e6);
  }
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  return String(value);
}

/** Evaluate a single sheet (orphan / no workbook) with its own formulas. */
export function evaluateSheetOverlay(name: string, markdown: string): SheetOverlay {
  const result = evaluateWorkbookOverlayFromManifest(
    { title: name, sheets: [name] },
    [{ name, markdown }],
  );
  return result.sheets[0] ?? { name, cells: {} };
}
