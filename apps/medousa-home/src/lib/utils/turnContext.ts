import type { HostTurnContext, TurnResourceReference } from "$lib/types/generated/daemon_api";
import type { VaultNoteContextScope } from "./vaultNoteBridge";

/** References are evidence. Actual Code binding stays with the existing binding API. */
export function resourceContext(source: string, references: TurnResourceReference[]): HostTurnContext | null {
  if (!references.length) return null;
  if (references.length > 16) throw new Error("Attach up to 16 resource references to one request.");
  return { source, references, diagnostics: [], related_resources: [] };
}
export function noteResource(scope: VaultNoteContextScope, workshopId: string): TurnResourceReference {
  if (!scope.rootId || scope.workshopId !== workshopId) throw new Error("This note belongs to a different vault or workshop. Attach it again.");
  return { kind: "vault_note", root_id: scope.rootId, path: scope.path, title: scope.title,
    selection: scope.selection ? { text: scope.selection.text } : null };
}
export function composerContext(source: string, references: TurnResourceReference[], note: VaultNoteContextScope | null, workshopId: string): HostTurnContext | null {
  return resourceContext(source, [...references, ...(note ? [noteResource(note, workshopId)] : [])]);
}
export function referenceKey(ref: TurnResourceReference): string {
  return ref.kind === "vault_note" ? `note:${ref.root_id}:${ref.path}` : ref.kind === "code_project" ? `code:${ref.work_id}` : `create:${ref.document_id}:${ref.snapshot_id ?? ""}:${(ref.selected_ids ?? []).join(",")}`;
}
