<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { Code2, X, ArrowUpRight } from "@lucide/svelte";
  import TaskDialogFrame from "$lib/components/tasks/TaskDialogFrame.svelte";
  import { creativeBridgeApi, designBuildPrompt } from "$lib/create/creativeBridge";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import type { CreativeProjectLease } from "$lib/create/creativeWorkspace.svelte";
  import type { CreativeDocument, CreativeNode } from "$lib/types/generated/daemon_api";
  import type { ItemProjection } from "$lib/forge";
  let { lease, document, frame, disabled, capture }: { lease: CreativeProjectLease; document: CreativeDocument; frame: CreativeNode | undefined; disabled: boolean; capture: (revision: number, ids: string[]) => Promise<string> } = $props();
  let open = $state(false), busy = $state(false), loading = $state(false), error = $state("");
  let projects = $state.raw<ItemProjection[]>([]), workId = $state(""), direction = $state("");
  let sessionId = $state(""), uncertain = $state(false);
  const scope = untrack(() => lease.api.scope);
  const documentId = untrack(() => document.id);
  const linked = $derived(Boolean(frame?.design_source?.work_id));
  let alive = true; onDestroy(() => alive = false);
  async function show() {
    if (!frame || frame.locked || disabled) return;
    open = true; loading = true; error = "";
    workId = frame.design_source?.work_id ?? "";
    try { projects = await creativeBridgeApi(scope).projects(); if (!workId && projects.length === 1) workId = projects[0].id; }
    catch (e) { error = e instanceof Error ? e.message : String(e); }
    finally { loading = false; }
  }
  function current() { return alive && activeWorkshopId() === scope && lease.api.scope === scope && document.id === documentId && lease.session.document?.id === documentId; }
  function openConversation() { if (current() && sessionId) { open = false; shellTabs.openChat(sessionId, { activate: true }); } }
  async function build() {
    if (!frame || frame.locked || disabled || busy || !workId || uncertain) return;
    busy = true; error = "";
    try {
      const api = creativeBridgeApi(scope), frameId = frame.id;
      if (!current()) throw new Error("The workshop changed");
      if (codeWorkspace.tabsFor(workId).some(tab => codeWorkspace.isDirty(tab))) throw new Error("Save your open Code files before handing this design to Medousa.");
      const project = await api.project(workId);
      if (!current()) throw new Error("The workshop changed before the handoff");
      if (project.workspace_present === false) throw new Error("Open this project in Code and prepare its working copy first.");
      const doc = lease.session.document;
      if (!doc || lease.session.dirty || lease.session.busy) throw new Error("Finish saving the design first.");
      const existing = doc.scene.nodes.find(n => n.id === frameId);
      if (!existing || existing.content.kind !== "frame") throw new Error("The selected frame was removed.");
      // The bound project remains authoritative. The stored design link is advisory.
      sessionId = existing.design_source?.work_id === workId ? existing.design_source?.session_id ?? "" : "";
      if (sessionId) {
        try { if ((await api.binding(sessionId)).work_id !== workId) sessionId = ""; }
        catch { sessionId = ""; } // A removed conversation does not strand the saved design.
      }
      if (!sessionId) sessionId = (await api.newSession(`${linked ? "Update" : "Build"} · ${document.title}`)).session_id;
      if (!current()) throw new Error("The workshop changed before the handoff");
      await api.bind(sessionId, workId);
      if (!current()) throw new Error("The workshop changed before the handoff");
      if (lease.session.document?.revision !== doc.revision || lease.session.dirty || lease.session.busy) throw new Error("The design changed while preparing the handoff. Try again after it saves.");
      const source = { url: "", viewport_width: Math.max(1, Math.min(8192, Math.ceil(existing.transform.width))), viewport_height: Math.max(1, Math.min(8192, Math.ceil(existing.transform.height))), captured_at_ms: Date.now(), ...existing.design_source, work_id: workId, session_id: sessionId };
      if (JSON.stringify(existing.design_source) !== JSON.stringify(source)) {
        lease.session.enqueue("Link design to Code", [{ op: "put_node", node: { ...existing, design_source: source } }]); await lease.session.save();
      }
      if (!current()) throw new Error("The project changed before the handoff");
      const saved = lease.session.document!;
      const visual = await capture(saved.revision, [frameId]);
      if (!current()) throw new Error("The workshop changed during capture");
      const context = await api.context(saved.id, frameId, saved.revision, visual, sessionId);
      if (!current() || lease.session.document?.revision !== saved.revision) throw new Error("The design changed before the handoff. Capture it again.");
      const prompt = designBuildPrompt(saved, saved.scene.nodes.find(n => n.id === frameId)!, direction, context.context);
      // Never silently resubmit a timed-out runtime admission: the session can recover it.
      uncertain = true;
      await api.start(sessionId, workId, prompt, context.media_refs);
      uncertain = false; openConversation();
    } catch (e) { error = (e instanceof Error ? e.message : String(e)) + (uncertain ? " Open the conversation to check whether the request started before sending another." : ""); }
    finally { busy = false; }
  }
</script>

{#if frame}<button type="button" aria-label={linked ? "Update Code from frame" : "Build frame in Code"} disabled={disabled || frame.locked} onclick={() => void show()}><Code2 size={14}/>{linked ? "Update Code" : "Build in Code"}</button>{/if}
{#if open}
  <TaskDialogFrame compact label="Build design in Code" onClose={() => { if (!busy) open = false; }}>
    <form class="create-code-bridge" onsubmit={e => { e.preventDefault(); void build(); }}>
      <header><div><h2>{linked ? "Update Code" : "Build in Code"}</h2><p>{frame?.name || "Selected frame"} · {document.title}</p></div><button type="button" aria-label="Close Code handoff" disabled={busy} onclick={() => open = false}><X size={16}/></button></header>
      <label>Code project<select aria-label="Code project" bind:value={workId} disabled={busy || loading || uncertain}><option value="">{loading ? "Loading projects…" : "Choose a project"}</option>{#each projects as project}<option value={project.id}>{project.title}</option>{/each}</select></label>
      {#if !loading && !projects.length}<p>Open a repository in Code first, then select it here.</p>{/if}
      <label>What should Medousa build?<textarea aria-label="Build instructions" bind:value={direction} maxlength="8000" rows="3" placeholder="Match this design and reuse our existing components…" disabled={busy || uncertain}></textarea></label>
      <p>Medousa receives a saved snapshot and edits the real repo using the existing Code runtime. Later canvas edits wait for your next Update Code request.</p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <footer>{#if uncertain}<button type="button" onclick={openConversation}><ArrowUpRight size={14}/>Check conversation</button>{:else}<button class="primary" type="submit" disabled={busy || loading || !workId || disabled}>{busy ? "Preparing design…" : linked ? "Update Code" : "Build with Medousa"}</button>{/if}</footer>
    </form>
  </TaskDialogFrame>
{/if}
<style>
  .create-code-bridge { padding: 22px; display: grid; gap: 16px; font-size: 12px; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  h2 { margin: 0 0 5px; font-size: 16px; font-weight: 500; }
  p { margin: 0; opacity: .65; line-height: 1.5; }
  label { display: grid; gap: 6px; }
  select, textarea { width: 100%; padding: 9px; border: 1px solid var(--create-line, #ffffff22); background: var(--create-soft, #ffffff08); color: inherit; border-radius: 7px; font: inherit; }
  footer { display: flex; justify-content: flex-end; }
  button { display: inline-flex; align-items: center; gap: 5px; padding: 7px 10px; border-radius: 6px; }
  .primary { background: var(--create-text, #e7e6df); color: var(--create-raised, #171714); }
  .error { opacity: 1; color: #d76956; }
</style>
