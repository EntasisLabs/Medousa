import { acquireCreativeProject } from "./creativeWorkspace.svelte";
import { createBrowser } from "./createBrowser.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import type { CreativeOperation, CreativeDocument } from "$lib/types/generated/daemon_api";

export function acknowledgeCreativeProject(scope: string, document: CreativeDocument) {
  createBrowser(scope).acknowledge(document);
  for (const tab of shellTabs.tabs) {
    if (tab.kind === "create" && tab.documentId === document.id && tab.workshopId === scope && tab.title !== document.title)
      shellTabs.patchTitle(tab.id, document.title);
  }
}

/** Use the canvas's shared session: metadata edits serialize with its autosaves. */
export async function editCreativeProject(scope: string, id: string, operation: Extract<CreativeOperation, { op: "rename" | "pin" }>) {
  const lease = acquireCreativeProject(scope, id);
  try {
    if (!lease.session.document) await lease.session.load();
    if (!lease.session.canEdit) throw new Error("Open this project and finish its pending edit first");
    lease.session.enqueue(operation.op === "rename" ? "Rename project" : "Pin project", [operation]);
    await lease.session.save();
    acknowledgeCreativeProject(scope, lease.session.document!);
  } finally { lease.release(); }
}

export async function deleteCreativeProject(scope: string, id: string) {
  const lease = acquireCreativeProject(scope, id);
  try {
    if (!lease.session.document) await lease.session.load();
    await lease.session.remove(lease.api.delete);
    createBrowser(scope).forget(id);
    for (const tab of [...shellTabs.tabs]) {
      if (tab.kind === "create" && tab.documentId === id && tab.workshopId === scope)
        shellTabs.close(tab.id);
    }
  } finally { lease.release(); }
}
