/** Resolve medousa-view host clicks without configure swallowing Copy CSV. */

export type MedousaViewHostClick =
  | { kind: "copyCsv"; payload: string }
  | { kind: "configure"; index: number }
  | { kind: "none" };

/**
 * Priority: Copy CSV → Configure button only → none.
 * Do not match wrapper `[data-edit-view-index]` — that steals CSV clicks.
 */
export function resolveMedousaViewHostClick(target: Element | null): MedousaViewHostClick {
  if (!target || !(target instanceof Element)) return { kind: "none" };

  const copyCsv = target.closest("[data-copy-view-csv]");
  if (copyCsv) {
    const payload =
      copyCsv.getAttribute("data-view-csv") ??
      copyCsv.getAttribute("data-copy-view-csv") ??
      "";
    return { kind: "copyCsv", payload };
  }

  const configure = target.closest(".medousa-view-configure");
  if (configure) {
    const raw =
      configure.getAttribute("data-edit-view-index") ??
      configure.closest("[data-edit-view-index]")?.getAttribute("data-edit-view-index");
    const index = raw == null ? NaN : Number(raw);
    if (Number.isFinite(index)) return { kind: "configure", index };
  }

  return { kind: "none" };
}
