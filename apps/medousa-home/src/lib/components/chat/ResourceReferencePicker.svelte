<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { untrack } from "svelte";
  import { X, FileText, Code2, Shapes } from "@lucide/svelte";
  import { VaultSearchCatalog } from "$lib/vault/vaultSearchCatalog.svelte";
  import { listUndertakings } from "$lib/forge";
  import { creativeApi } from "$lib/create/creativeApi";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { referenceKey } from "$lib/utils/turnContext";
  import { referenceToken, removeReferenceToken, referenceLabel } from "$lib/context/resourceReferences";
  import type { TurnResourceReference } from "$lib/types/generated/daemon_api";
  let { value = $bindable(""), references = $bindable<TurnResourceReference[]>([]), element, enabled = true, onChange, onBindProject }: {
    value: string; references: TurnResourceReference[]; element: HTMLInputElement | HTMLTextAreaElement | null | undefined;
    enabled?: boolean; onChange?: () => void; onBindProject?: (id: string) => Promise<void>;
  } = $props();
  const notes = new VaultSearchCatalog();
  let cursor = $state(0), index = $state(0), dismissed = $state(false);
  let items = $state<{ reference: TurnResourceReference; detail: string }[]>([]);
  let loading = $state(false), error = $state<string | null>(null);
  let binding = $state(false), boundProject = $state<string | null>(null), bindingError = $state<string | null>(null);
  let position = $state({ left: 16, top: 100, width: 320 });
  const token = $derived(enabled ? referenceToken(value, cursor) : null);
  const open = $derived(Boolean(token) && !dismissed);
  $effect(() => {
    if (!element) return;
    const update = () => { cursor = element?.selectionStart ?? value.length; dismissed = false; };
    element.addEventListener("input", update); element.addEventListener("click", update); element.addEventListener("select", update);
    return () => { element?.removeEventListener("input", update); element?.removeEventListener("click", update); element?.removeEventListener("select", update); };
  });
  $effect(() => {
    const currentToken = token, scope = activeWorkshopId();
    if (!open || !currentToken || !element) return;
    const rect = element.getBoundingClientRect();
    const height = Math.min(300, Math.max(160, window.innerHeight - 80));
    position = { left: Math.max(8, Math.min(rect.left, window.innerWidth - 368)), top: rect.bottom + height + 16 < window.innerHeight ? rect.bottom + 8 : Math.max(8, rect.top - height - 8), width: Math.min(360, window.innerWidth - 16) };
    let active = true;
    loading = true; items = []; index = 0; error = null;
    const timer = setTimeout(() => { void (async () => {
      const query = currentToken.query.toLocaleLowerCase();
      const result: typeof items = [];
      const failures: string[] = [];
      await Promise.all([
        (async () => { if (!["all", "note"].includes(currentToken.kind)) return;
          await notes.search(currentToken.query);
          result.push(...notes.resultsFor(currentToken.query).slice(0, 8).map(hit => ({ reference: { kind: "vault_note" as const, root_id: hit.rootId, path: hit.note.path, title: hit.note.title }, detail: `${hit.rootLabel} · ${hit.note.path}` })));
          if (notes.error) failures.push(notes.error);
        })(),
        (async () => { if (!["all", "project"].includes(currentToken.kind)) return;
          try { const projects = await listUndertakings(); result.push(...projects.filter(p => !query || `${p.title} ${p.id}`.toLocaleLowerCase().includes(query)).slice(0, 8).map(p => ({ reference: { kind: "code_project" as const, work_id: p.id, title: p.title }, detail: "Code project · reference" }))); }
          catch { failures.push("Code projects unavailable"); }
        })(),
        (async () => { if (!["all", "frame"].includes(currentToken.kind)) return;
          try { const docs = (await creativeApi(scope).list(undefined, 100)).projects;
            const matching = docs.filter(p => !query || p.title.toLocaleLowerCase().includes(query));
            if (currentToken.kind === "frame") {
              const pages = await Promise.all((matching.length ? matching : docs).slice(0, 8).map(async doc => ({ doc, page: await creativeApi(scope).get(doc.id) })));
              for (const { doc, page } of pages) result.push(...page.document.scene.nodes.filter(n => n.content.kind === "frame" && n.visible && (!query || matching.some(d => d.id === doc.id) || (n.name ?? "").toLocaleLowerCase().includes(query))).slice(0, 8).map(n => ({ reference: { kind: "creative_selection" as const, document_id: doc.id, title: `${doc.title} · ${n.name || n.id}`, selected_ids: [n.id] }, detail: "Create frame · reference" })));
            } else result.push(...matching.slice(0, 8).map(p => ({ reference: { kind: "creative_selection" as const, document_id: p.id, title: p.title, selected_ids: [] }, detail: "Create project · reference" })));
          } catch { failures.push("Create projects unavailable"); }
        })(),
      ]);
      if (!active || activeWorkshopId() !== scope) return;
      const attached = new Set(untrack(() => references.map(referenceKey)));
      items = result.filter(item => !attached.has(referenceKey(item.reference))).sort((a, b) => referenceLabel(a.reference).localeCompare(referenceLabel(b.reference))).slice(0, 18);
      error = failures.join(" · ") || null; loading = false;
    })(); }, 100);
    return () => { active = false; clearTimeout(timer); };
  });
  async function bindProject(workId: string) {
    if (!onBindProject || binding) return;
    binding = true; bindingError = null;
    try { await onBindProject(workId); boundProject = workId; }
    catch (cause) { bindingError = String(cause); }
    finally { binding = false; }
  }
  $effect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Element && !event.target.closest(".resource-picker") && event.target !== element) dismissed = true;
    };
    window.addEventListener("pointerdown", dismiss);
    return () => window.removeEventListener("pointerdown", dismiss);
  });
  function select(reference: TurnResourceReference) {
    if (!token || references.length >= 16) return;
    const next = removeReferenceToken(value, token);
    references = [...references, reference]; value = next.value; cursor = next.cursor; dismissed = true; onChange?.();
    requestAnimationFrame(() => { element?.focus(); element?.setSelectionRange(cursor, cursor); });
  }
  $effect(() => {
    if (!element) return;
    const input = element;
    const handle = (inputEvent: Event) => {
      const event = inputEvent as KeyboardEvent;
      if (!open || event.isComposing || !["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(event.key)) return;
      event.preventDefault(); event.stopImmediatePropagation();
      if (event.key === "Escape") dismissed = true;
      else if (event.key === "Enter" || event.key === "Tab") { if (items[index]) select(items[index].reference); }
      else if (items.length) index = (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
    };
    input.addEventListener("keydown", handle, true);
    return () => input.removeEventListener("keydown", handle, true);
  });
</script>
{#if references.length}
  <div class="resource-chips" aria-label="Referenced resources">
    {#each references as reference, i (referenceKey(reference))}
      <span class="resource-chip" title={reference.kind === 'vault_note' ? `${reference.root_id} · ${reference.path}` : 'Reference context'}>
        {#if reference.kind === 'vault_note'}<FileText size={12}/>{:else if reference.kind === 'code_project'}<Code2 size={12}/>{:else}<Shapes size={12}/>{/if}
        <span>{referenceLabel(reference)}{reference.kind === 'vault_note' ? ` · ${reference.root_id}` : ''}</span>
        {#if reference.kind === 'code_project' && onBindProject}<button type="button" disabled={!enabled || binding} onclick={() => void bindProject(reference.work_id)}>{boundProject === reference.work_id ? "Bound" : "Work here"}</button>{/if}
        <button type="button" aria-label={`Remove reference ${referenceLabel(reference)}`} disabled={!enabled} onclick={() => references = references.filter((_, index) => index !== i)}><X size={12}/></button>
      </span>
    {/each}
  </div>
{/if}
{#if bindingError}<p class="resource-error" role="alert">{bindingError}</p>{/if}
{#if open}
  <BodyPortal><div class="resource-picker" style:left={`${position.left}px`} style:top={`${position.top}px`} style:width={`${position.width}px`} role="listbox" aria-label="Attach a note or project">
    <p>Reference a note or project</p>
    {#each items as item, i (referenceKey(item.reference))}
      <button type="button" role="option" aria-selected={index === i} class:highlighted={index === i} onpointerdown={event => event.preventDefault()} onpointermove={() => index = i} onclick={() => select(item.reference)}>
        <span>{referenceLabel(item.reference)}</span><small>{item.detail}</small>
      </button>
    {:else}<p>{loading ? 'Searching…' : 'No matching resources'}</p>{/each}
    {#if error}<p role="status">{error}</p>{/if}
  </div></BodyPortal>
{/if}
<style>
  .resource-error { color: rgb(var(--theme-text) / .7); font-size: 11px; margin: 4px 12px; }
  .resource-chips { display: flex; gap: 6px; flex-wrap: wrap; padding: 6px 12px; }
  .resource-chip { display: inline-flex; gap: 6px; align-items: center; min-width: 0; max-width: 100%; border: 1px solid rgb(var(--theme-border) / .4); background: rgb(var(--theme-text) / .035); color: rgb(var(--theme-text) / .75); border-radius: 7px; padding: 4px 6px; font-size: 11px; }
  .resource-chip > span { max-width: 230px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .resource-chip button { display: flex; align-items: center; padding: 2px; color: inherit; }
  .resource-picker { position: fixed; z-index: 10020; padding: 6px; max-height: 300px; overflow-y: auto; border: 1px solid rgb(var(--theme-border) / .4); background: rgb(var(--theme-pane)); color: rgb(var(--theme-text)); border-radius: 10px; box-shadow: 0 10px 32px #0005; }
  .resource-picker p { margin: 4px 8px 8px; color: rgb(var(--theme-text) / .55); font-size: 11px; }
  .resource-picker button { display: flex; flex-direction: column; gap: 3px; width: 100%; text-align: left; padding: 8px; border-radius: 6px; font-size: 12px; }
  .resource-picker button.highlighted { background: rgb(var(--theme-text) / .07); }
  .resource-picker small { color: rgb(var(--theme-text) / .5); font-size: 10px; }
</style>
