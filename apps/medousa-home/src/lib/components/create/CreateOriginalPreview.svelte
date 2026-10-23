<script lang="ts">
  import { onDestroy } from "svelte";
  import { Scan, X, ChevronLeft, ChevronRight } from "@lucide/svelte";
  import TaskDialogFrame from "$lib/components/tasks/TaskDialogFrame.svelte";
  import { originalReferences } from "$lib/create/creativeReferences";
  import { creativeAssets } from "$lib/create/creativeAssets";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import type { CreativeProjectLease } from "$lib/create/creativeWorkspace.svelte";
  import type { CreativeDocument } from "$lib/types/generated/daemon_api";
  let { lease, document, selected, onframe }: { lease: CreativeProjectLease; document: CreativeDocument; selected: string[]; onframe: (id: string) => void } = $props();
  const references = $derived(originalReferences(document));
  let activeFrame = $state("");
  $effect(() => {
    let node = document.scene.nodes.find(n => selected.includes(n.id));
    const visited = new Set<string>();
    while (node && !visited.has(node.id)) {
      visited.add(node.id);
      if (references.some(r => r.frame.id === node!.id)) { activeFrame = node.id; break; }
      node = document.scene.nodes.find(n => n.id === node!.parent_id);
    }
  });
  const index = $derived(Math.max(0, references.findIndex(r => r.frame.id === activeFrame)));
  let open = $state(false), url = $state(""), error = $state("");
  let alive = true, epoch = 0, cache: ReturnType<typeof creativeAssets> | null = null;
  onDestroy(() => { alive = false; epoch++; cache?.dispose(); });
  $effect(() => {
    const ref = references[index], show = open;
    if (!show || !ref) return;
    const request = ++epoch, scope = lease.api.scope;
    url = ""; error = "";
    cache ??= creativeAssets(lease.api, document.id);
    cache.get(ref.asset).then(value => {
      if (alive && open && request === epoch && activeWorkshopId() === scope) { url = value; cache?.retainOnly(new Set([ref.asset])); }
    }).catch(e => { if (alive && request === epoch) error = e instanceof Error ? e.message : String(e); });
  });
  function close() { open = false; epoch++; cache?.dispose(); cache = null; url = ""; }
</script>
{#if references.length}
  <div class="reference-controls">
    {#if references.length > 1}<div class="frame-nav" role="group" aria-label="Page frames"><button type="button" aria-label="Previous frame" disabled={index === 0} onclick={() => onframe(references[index - 1].frame.id)}><ChevronLeft size={14}/></button><select aria-label="Active page frame" value={references[index].frame.id} onchange={e => onframe(e.currentTarget.value)}>{#each references as ref, i}<option value={ref.frame.id}>{i + 1} / {references.length}</option>{/each}</select><button type="button" aria-label="Next frame" disabled={index === references.length - 1} onclick={() => onframe(references[index + 1].frame.id)}><ChevronRight size={14}/></button></div>{/if}
    <button type="button" onclick={() => open = true}><Scan size={14}/>View original</button>
  </div>
{/if}
{#if open && references[index]}
  <TaskDialogFrame label="Original page reference" onClose={close}>
    <header><div><strong>Original capture</strong><span>{references[index].frame.name ?? "Page"} · Read only</span></div><button type="button" aria-label="Close original preview" onclick={close}><X size={18}/></button></header>
    <div class="original-image">{#if error}<p role="alert">{error}</p>{:else if url}<img src={url} alt="Original rendered page before canvas edits"/>{:else}<p role="status">Loading original…</p>{/if}</div>
  </TaskDialogFrame>
{/if}
<style>
  .reference-controls,.frame-nav { display:flex;align-items:center;gap:3px;min-width:0; }
  button { display:inline-flex;align-items:center;gap:5px;padding:7px;border-radius:6px;font-size:11px; }
  button:hover { background:#ffffff0b; } button:disabled { opacity:.35; }
  select { background:transparent;color:inherit;border:0;font-size:11px;max-width:80px; }
  header { display:flex;align-items:center;justify-content:space-between;padding:15px 18px;border-bottom:1px solid #ffffff15;flex:none; }
  header div { display:grid;gap:4px; } strong { font-size:13px;font-weight:500; } span { opacity:.5;font-size:11px; }
  .original-image { flex:1;min-height:0;overflow:auto;display:grid;place-items:center;padding:16px;background:#0002; }
  img { display:block;max-width:100%;max-height:100%;object-fit:contain; } p { opacity:.6;font-size:12px; }
  @media(pointer:coarse) { button,select { min-height:44px; } }
</style>
