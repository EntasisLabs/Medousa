import { deleteGraphemeScript, renameGraphemeScript, saveGraphemeScript } from "$lib/daemon";
import type { ScriptEditorTab } from "$lib/stores/graphemeScriptEditor.svelte";
import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { workshop } from "$lib/stores/workshop.svelte";

function applySavedName(tabId: string, entry: { id: string; name: string; version: number }) {
  graphemeScriptEditor.tabs = graphemeScriptEditor.tabs.map((tab) =>
    tab.tabId === tabId
      ? {
          ...tab,
          scriptId: entry.id,
          name: entry.name,
          version: entry.version,
          dirty: false,
        }
      : tab,
  );
}

/** Persist a rename for a saved script; drafts only update local tab state. */
export async function persistScriptName(tab: ScriptEditorTab, name: string): Promise<void> {
  const trimmed = name.trim() || "Untitled script";
  graphemeScriptEditor.patchTab(tab.tabId, { name: trimmed });
  lmeWorkspace.syncScriptTabFromEditor({ activate: false });

  if (!tab.scriptId) return;

  if (tab.body.trim()) {
    const response = await saveGraphemeScript({
      id: tab.scriptId,
      name: trimmed,
      body: tab.body,
      intent: tab.intent.trim() || null,
      tags: tab.tags,
    });
    applySavedName(tab.tabId, response.script);
  } else {
    const response = await renameGraphemeScript(tab.scriptId, trimmed);
    applySavedName(tab.tabId, response.script);
  }

  lmeWorkspace.syncScriptTabFromEditor({ activate: false });
  await workshop.refreshModulesAndScripts();
}

export async function renameScriptById(scriptId: string, name: string): Promise<void> {
  const trimmed = name.trim() || "Untitled script";
  const response = await renameGraphemeScript(scriptId, trimmed);

  for (const tab of graphemeScriptEditor.tabs) {
    if (tab.scriptId !== scriptId) continue;
    applySavedName(tab.tabId, response.script);
  }
  lmeWorkspace.syncScriptTabFromEditor({ activate: false });
  await workshop.refreshModulesAndScripts();
}

export async function deleteScriptById(scriptId: string): Promise<void> {
  await deleteGraphemeScript(scriptId);

  const editorTabIds = new Set(
    graphemeScriptEditor.tabs
      .filter((tab) => tab.scriptId === scriptId)
      .map((tab) => tab.tabId),
  );

  for (const tab of [...lmeWorkspace.tabs]) {
    if (tab.kind === "script" && editorTabIds.has(tab.scriptTabId)) {
      await lmeWorkspace.closeTab(tab.tabId);
    }
  }

  for (const tabId of editorTabIds) {
    if (graphemeScriptEditor.tabs.some((tab) => tab.tabId === tabId)) {
      graphemeScriptEditor.closeTab(tabId);
    }
  }

  await workshop.refreshModulesAndScripts();
}

export async function closeActiveScriptTab(): Promise<void> {
  const lme = lmeWorkspace.activeTab;
  if (lme?.kind === "script") {
    await lmeWorkspace.closeTab(lme.tabId);
    return;
  }
  const tab = graphemeScriptEditor.activeTab;
  if (tab) graphemeScriptEditor.closeTab(tab.tabId);
}

/** True for plain form fields; false for CodeMirror / other non-input focus. */
export function isPlainTextEditingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
}
