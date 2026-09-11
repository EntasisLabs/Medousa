import { activeWorkshopId, workshopScopedStorageKey } from "$lib/utils/workshopLocality";
export type WorkingReviewAnchor = { path: string; side: "new" | "old"; line: number; content: string; baselineOid: string; workingDigest: string | null };
export type WorkingReviewNote = WorkingReviewAnchor & { id: string; body: string };
export function reviewNotesKey(workId: string): string {
  return workshopScopedStorageKey(`medousa:review-notes:${workId}`, activeWorkshopId());
}
export function readReviewNotes(key: string): WorkingReviewNote[] {
  try {
    const notes: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    if (!Array.isArray(notes)) return [];
    return notes.filter((n): n is WorkingReviewNote => n && typeof n.id === "string" && typeof n.path === "string" && typeof n.body === "string" && typeof n.content === "string" && typeof n.baselineOid === "string" && (n.workingDigest === null || typeof n.workingDigest === "string") && (n.side === "new" || n.side === "old") && Number.isInteger(n.line) && n.line > 0).slice(0, 200);
  } catch { return []; }
}
export function writeReviewNotes(key: string, notes: WorkingReviewNote[]): void {
  localStorage.setItem(key, JSON.stringify(notes));
}
export function reviewNotesPrompt(title: string, notes: WorkingReviewNote[]): string {
  return `Revise the changes in ${title} using these review notes. These refer to the captured versions below; inspect current files and reconcile any later edits before applying them.\n\n` + notes.map((n) =>
    `${n.path}:${n.line} (${n.side} side; baseline ${n.baselineOid}; working digest ${n.workingDigest ?? "deleted"})\nReviewed line: ${JSON.stringify(n.content)}\nComment: ${n.body}`,
  ).join("\n\n");
}
