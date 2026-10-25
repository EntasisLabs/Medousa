import type { TurnResourceReference } from "$lib/types/generated/daemon_api";

export interface ReferenceToken { start: number; end: number; query: string; kind: "all" | "note" | "project" | "frame" }
/** Caret-scoped references; a leading Spotlight @address is handled by its recipient picker. */
export function referenceToken(value: string, cursor: number): ReferenceToken | null {
  const before = value.slice(0, cursor);
  const at = /(?:^|\s)(@[^\s@]*)$/.exec(before);
  if (at) return { start: cursor - at[1].length, end: cursor, query: at[1].slice(1), kind: "all" };
  const slash = /(?:^|\s)(\/(note|project|frame)(?:\s+([^\n]*))?)$/.exec(before);
  return slash ? { start: cursor - slash[1].length, end: cursor, query: slash[3] ?? "", kind: slash[2] as ReferenceToken["kind"] } : null;
}
export function removeReferenceToken(value: string, token: ReferenceToken): { value: string; cursor: number } {
  return { value: value.slice(0, token.start) + value.slice(token.end), cursor: token.start };
}
export function referenceLabel(reference: TurnResourceReference): string {
  return reference.title || (reference.kind === "vault_note" ? reference.path : reference.kind === "code_project" ? reference.work_id : reference.document_id);
}
