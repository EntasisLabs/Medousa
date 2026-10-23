<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { X, Scan, ArrowUpRight } from "@lucide/svelte";
  import TaskDialogFrame from "$lib/components/tasks/TaskDialogFrame.svelte";
  import { creativeBridgeApi, importBrowserDesign, newDesignImportId } from "$lib/create/creativeBridge";
  import { createBrowser } from "$lib/create/createBrowser.svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import type { BrowserDesignCapture, BrowserDesignPageCapture, CreativeDocument } from "$lib/types/generated/daemon_api";
  import type { ItemProjection } from "$lib/forge";
  let { scope, open = $bindable(false), initialUrl = "", onimport }: { scope: string; open?: boolean; initialUrl?: string; onimport: (document: CreativeDocument) => void } = $props();
  let loading = $state(false), busy = $state(false), error = $state(""), warnings = $state<string[]>([]);
  let projects = $state.raw<ItemProjection[]>([]), worlds = $state.raw<{ world_id: string; title: string; url: string; run_state: string }[]>([]);
  let worldId = $state(""), workId = $state(""), url = $state("");
  let fullPage = $state(true);
  let capture = $state.raw<BrowserDesignCapture | BrowserDesignPageCapture | null>(null), importId = "", generation = 0;
  let alive = true; onDestroy(() => alive = false);
  let wasOpen = false, lastInitialUrl = "";
  let stateScope = untrack(() => scope);
  $effect(() => {
    const next = scope;
    untrack(() => {
      if (next === stateScope) return;
      stateScope = next; generation++; reset(); projects = []; worlds = []; worldId = ""; workId = ""; url = initialUrl;
      if (open) void load();
    });
  });
  $effect(() => { if (open && !wasOpen) { if (initialUrl !== lastInitialUrl) { reset(); url = initialUrl; lastInitialUrl = initialUrl; } void load(); } wasOpen = open; });
  function current(owner = scope) { return alive && scope === owner && activeWorkshopId() === owner; }
  async function load() {
    loading = true; error = ""; const request = ++generation, owner = scope;
    try {
      const api = creativeBridgeApi(owner), [p, w] = await Promise.all([api.projects(), api.worlds()]);
      if (request !== generation || !current(owner)) return;
      projects = p; worlds = w.worlds.filter(world => world.run_state === "running");
      if (!worldId && worlds.length === 1) worldId = worlds[0].world_id;
    } catch (e) { if (request === generation && current(owner)) error = e instanceof Error ? e.message : String(e); }
    finally { if (request === generation) loading = false; }
  }
  async function projectPreview() {
    capture = null; importId = "";
    if (!workId) return;
    const chosen = workId, owner = scope;
    try {
      const runs = await creativeBridgeApi(owner).runs(chosen);
      if (chosen === workId && current(owner)) url = runs.runs.find(run => run.ready_url && !run.terminal)?.ready_url ?? "";
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
  }
  function reset() { capture = null; importId = ""; warnings = []; imported = null; }
  async function capturePage() {
    if (busy || !current()) return;
    busy = true; error = "";
    const owner = scope, valid = () => current(owner);
    try {
      const api = creativeBridgeApi(owner);
      if (!capture) {
        if (url.trim()) {
          const address = new URL(url.trim());
          if (!["http:", "https:"].includes(address.protocol)) throw new Error("Use an HTTP or HTTPS dev-server URL.");
          worldId = (await api.openPreview(address.toString())).world.world_id;
          worlds = [...worlds.filter(w => w.world_id !== worldId), { world_id: worldId, title: "Create preview", url: address.toString(), run_state: "running" }];
          url = "";
          if (!valid()) throw new Error("The workshop changed while opening the preview");
        }
        if (!worldId) throw new Error("Choose a browser page or enter a running dev-server URL.");
        const captured = fullPage ? await api.capturePage(worldId) : await api.capture(worldId);
        if (!valid()) throw new Error("The workshop changed during capture");
        capture = captured; importId = newDesignImportId(); warnings = [...new Set("slices" in capture ? capture.slices.flatMap(s => s.capture.warnings) : capture.warnings)];
      }
      const document = await importBrowserDesign(owner, importId, capture, workId || null, valid);
      if (!valid()) throw new Error("The workshop changed during import");
      createBrowser(owner).acknowledge(document);
      // Keep any import limitations visible until the user opens the design.
      if (warnings.length) { imported = document; return; }
      finish(document);
    } catch (e) { error = e instanceof Error ? e.message : String(e); }
    finally { busy = false; }
  }
  let imported = $state.raw<CreativeDocument | null>(null);
  function finish(document: CreativeDocument) { open = false; reset(); imported = null; onimport(document); }
</script>
{#if open}
  <TaskDialogFrame compact label="Import page into Create" onClose={() => { if (!busy) open = false; }}>
    <form class="create-page-import" onsubmit={e => { e.preventDefault(); void capturePage(); }}>
      <header><div><h2><Scan size={17}/>Import page</h2><p>Yoink a running preview into an editable canvas.</p></div><button type="button" aria-label="Close page import" disabled={busy} onclick={() => open = false}><X size={16}/></button></header>
      {#if imported}<p>The editable frames are saved. Use View original to peek at their captured reference.</p>{#if warnings.length}<ul>{#each warnings as warning}<li>{warning}</li>{/each}</ul>{/if}<footer><button type="button" class="primary" onclick={() => imported && finish(imported)}><ArrowUpRight size={14}/>Open design</button></footer>
      {:else}
        <label>Code project (optional)<select aria-label="Import Code project" bind:value={workId} disabled={busy || loading} onchange={() => void projectPreview()}><option value="">Choose a project…</option>{#each projects as project}<option value={project.id}>{project.title}</option>{/each}</select></label>
        <label>Workshop browser<select aria-label="Browser page to import" bind:value={worldId} disabled={busy || loading} onchange={() => { url = ""; reset(); }}><option value="">Choose a running page…</option>{#each worlds as world}<option value={world.world_id}>{world.title || "Browser page"}</option>{/each}</select></label>
        <label>Or open a dev-server URL<input aria-label="Dev-server URL" bind:value={url} type="url" placeholder="http://localhost:5173" disabled={busy} oninput={reset}/></label>
        <label>Capture<select aria-label="Capture area" bind:value={fullPage} disabled={busy} onchange={reset}><option value={true}>Full page · stacked frames</option><option value={false}>Current viewport</option></select></label>
        <p>The preview runs on your workshop. Text and simple surfaces become editable; complex graphics stay as image regions. Full page captures up to 12 screenfuls, restores the browser position, and keeps the original behind View original. Form values and interactive behavior aren’t turned into design controls.</p>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        <footer><button type="button" disabled={busy || loading} onclick={() => void load()}>Refresh pages</button><button class="primary" type="submit" disabled={busy || loading || !worldId && !url.trim()}>{busy ? "Importing page…" : error && capture ? "Retry import" : "Capture into Create"}</button></footer>
      {/if}
    </form>
  </TaskDialogFrame>
{/if}
<style>
  .create-page-import { padding: 22px; display: grid; gap: 16px; font-size: 12px; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  h2 { display: flex; align-items: center; gap: 7px; margin: 0 0 5px; font-size: 16px; font-weight: 500; }
  p, li { margin: 0; opacity: .65; line-height: 1.5; }
  ul { padding-left: 18px; margin: 0; }
  label { display: grid; gap: 6px; }
  select, input { width: 100%; padding: 9px; border: 1px solid var(--create-line, #ffffff22); background: var(--create-soft, #ffffff08); color: inherit; border-radius: 7px; font: inherit; }
  footer { display: flex; justify-content: flex-end; gap: 8px; }
  button { display: inline-flex; align-items: center; gap: 5px; padding: 7px 10px; border-radius: 6px; }
  .primary { background: var(--create-text, #e7e6df); color: var(--create-raised, #171714); }
  .error { opacity: 1; color: #d76956; }
</style>
