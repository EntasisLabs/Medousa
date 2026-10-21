<script lang="ts">
  import { onDestroy } from "svelte";
  import { Plus, Check, X } from "@lucide/svelte";
  import { pushBrowserPopoverOverlay, popBrowserPopoverOverlay } from "$lib/utils/browserPopoverOverlay";
  import { mutateHostedReview } from "$lib/code/hostedReviews";
  import type { HostedReview, ReviewProviderQuery, ReviewProviderResult } from "$lib/types/generated/daemon_api";
  let { kind, review, connectionId, query, onupdated }: {
    kind: "labels" | "reviewers"; review: HostedReview; connectionId: string | null;
    query: (request: ReviewProviderQuery["request"]) => Promise<ReviewProviderResult>; onupdated: () => Promise<void>;
  } = $props();
  type Option = { id: string; name: string; description?: string; color?: string | null };
  let position = $state({ left: 0, top: 0 });
  let open = $state(false), search = $state(""), busy = $state(false), loading = $state(false), error = $state("");
  let options = $state<Option[]>([]), cursor = $state<string | null>(null), dialog = $state<HTMLDialogElement | null>(null);
  let alive = true, epoch = 0;
  const selected = $derived(kind === "labels" ? (review.labels ?? []).map(label => label.id) : review.requested_reviewers ?? []);
  const selectedOptions: Option[] = $derived(kind === "labels" ? review.labels ?? [] : selected.map(id => ({ id, name: id })));
  const choices: Option[] = $derived([...new Map([...selectedOptions, ...options].map(option => [option.id, option])).values()].filter(option => option.id !== review.author || kind === "labels").filter(option => `${option.name} ${"description" in option ? option.description : ""}`.toLowerCase().includes(search.trim().toLowerCase())));
  const canAdd = $derived(review.actions.includes(kind === "labels" ? "add_labels" : "request_reviewers"));
  const canRemove = $derived(review.actions.includes(kind === "labels" ? "remove_label" : "remove_reviewers"));
  $effect(() => {
    if (!open || !dialog) return;
    dialog.showModal(); void pushBrowserPopoverOverlay();
    return () => { dialog?.close(); void popBrowserPopoverOverlay(); };
  });
  onDestroy(() => { alive = false; epoch++; });
  function dismissOutside(event: PointerEvent) {
    if (event.target !== dialog || busy || !dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) open = false;
  }
  async function load(more = false) {
    if (loading) return;
    const current = ++epoch;
    loading = true; error = "";
    try {
      const result = await query({ operation: kind === "labels" ? "labels" : "reviewer_candidates", review: review.reference, cursor: more ? cursor : null });
      if (!alive || current !== epoch) return;
      const rows = result.kind === "labels" ? result.labels : result.kind === "people" ? result.people : null;
      if (!rows) throw new Error("The provider returned unexpected picker options.");
      options = [...new Map([...(more ? options : []), ...rows].map(row => [row.id, row])).values()];
      cursor = result.kind === "labels" || result.kind === "people" ? result.next_cursor ?? null : null;
    } catch (cause) { if (alive && current === epoch) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (alive && current === epoch) loading = false; }
  }
  async function toggle(option: Option) {
    if (busy) return;
    const removing = selected.includes(option.id);
    if (removing ? !canRemove : !canAdd) return;
    busy = true; error = "";
    try {
      const request = kind === "labels"
        ? removing ? { operation: "remove_label" as const, review: review.reference, label: option.id } : { operation: "add_labels" as const, review: review.reference, labels: [option.id] }
        : { operation: removing ? "remove_reviewers" as const : "request_reviewers" as const, review: review.reference, reviewers: [option.id] };
      const result = await mutateHostedReview(review.reference.provider_id, review.reference.host, request, connectionId);
      if (!alive) return;
      if (result.kind !== "updated") throw new Error("The provider did not confirm the change.");
      await onupdated();
    } catch (cause) { if (alive) error = `${cause instanceof Error ? cause.message : String(cause)} Refresh the review to check the result before trying again.`; }
    finally { if (alive) busy = false; }
  }
</script>
<button type="button" class="add" aria-label={kind === "labels" ? "Edit labels" : "Choose reviewers"} onclick={(event) => { const rect = event.currentTarget.getBoundingClientRect(); position = { left: Math.max(16, Math.min(rect.right - 352, window.innerWidth - 368)), top: Math.max(16, Math.min(rect.bottom + 6, window.innerHeight - 400)) }; open = true; search = ""; void load(); }}><Plus size={14}/></button>
{#if open}
  <dialog onpointerdown={dismissOutside} style:left={`${position.left}px`} style:top={`${position.top}px`} bind:this={dialog} aria-label={kind === "labels" ? "Labels" : "Reviewers"} oncancel={(event) => { event.preventDefault(); if (!busy) open = false; }}>
    <header><h2>{kind === "labels" ? "Labels" : "Request reviewers"}</h2><button class="add" type="button" aria-label="Close picker" disabled={busy} onclick={() => open = false}><X size={15}/></button></header>
    <input aria-label={kind === "labels" ? "Search labels" : "Search reviewers"} bind:value={search} placeholder={kind === "labels" ? "Find a label…" : "Find a person…"}/>
    <div class="options" aria-busy={busy || loading}>
      {#each choices as option (option.id)}
        {@const checked = selected.includes(option.id)}
        <button class="option" type="button" aria-pressed={checked} disabled={busy || (checked ? !canRemove : !canAdd)} onclick={() => void toggle(option)}>
          {#if kind === "labels"}<span class="dot" style:background-color={/^[\da-f]{6}$/i.test(option.color ?? "") ? `#${option.color}` : undefined}></span>{:else}<span class="avatar">{option.name.slice(0, 2).toUpperCase()}</span>{/if}
          <span class="copy"><strong>{option.name}</strong>{#if "description" in option && option.description}<small>{option.description}</small>{/if}</span><span class="selection">{#if checked}<Check size={14}/>{/if}</span>
        </button>
      {:else}{#if !loading}<p>No matches in the loaded options.</p>{/if}{/each}
      {#if cursor && options.length < 1000}<button type="button" class="more" disabled={loading || busy} onclick={() => void load(true)}>Load more {kind}</button>{/if}
      {#if kind === "reviewers" && search.trim() && search.trim() !== review.author && !choices.some(option => option.id.toLowerCase() === search.trim().toLowerCase()) && canAdd}<button class="more" type="button" disabled={busy} onclick={() => void toggle({ id: search.trim(), name: search.trim() })}>Request {search.trim()}</button>{/if}
      {#if loading}<p role="status">Loading…</p>{/if}
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <footer>{busy ? "Saving…" : kind === "labels" ? "Changes save immediately." : "Select a person to request a review. Uncheck to remove a pending request."}</footer>
  </dialog>
{/if}
<style>
  button { color:inherit; cursor:pointer; border:0; background:transparent; }
  .add { display:inline-flex; align-items:center; justify-content:center; width:1.4rem; height:1.4rem; padding:0; border-radius:50%; color:rgb(var(--theme-text-secondary)); }
  .add:hover { background:rgb(var(--theme-text)/.07); color:rgb(var(--theme-text)); }
  dialog { position:fixed; margin:0; max-height:calc(100vh - 2rem); overflow:auto; width:min(22rem, calc(100vw - 2rem)); padding:.85rem; border:1px solid rgb(var(--theme-border)/.7); border-radius:.75rem; background:rgb(var(--theme-card)); color:rgb(var(--theme-text)); box-shadow:0 14px 60px rgb(0 0 0/.3); }
  dialog::backdrop { background:rgb(0 0 0/.15); }
  header { display:flex; align-items:center; justify-content:space-between; margin-bottom:.65rem; } h2 { font-size:.8rem; font-weight:550; margin:0; }
  input { width:100%; padding:.5rem .6rem; background:rgb(var(--theme-text)/.035); border:1px solid rgb(var(--theme-border)/.6); border-radius:.4rem; font-size:.75rem; color:inherit; }
  .options { max-height:17rem; overflow:auto; margin:.5rem -.25rem; } .option { display:flex; align-items:center; gap:.55rem; padding:.5rem; width:100%; text-align:left; border-radius:.4rem; } .option:hover { background:rgb(var(--theme-text)/.05); }
  .copy { flex:1; min-width:0; overflow-wrap:anywhere; } strong { font-size:.75rem; font-weight:500; } small { display:block; margin-top:.15rem; color:rgb(var(--theme-text-tertiary)); font-size:.6875rem; }
  .dot { width:.55rem; height:.55rem; border-radius:50%; background:rgb(var(--theme-text-tertiary)); flex-shrink:0; } .avatar { display:grid; place-items:center; width:1.4rem; height:1.4rem; border-radius:50%; background:rgb(var(--theme-text)/.07); font-size:.55rem; flex-shrink:0; } .selection { width:1rem; display:flex; }
  p,footer,.more { font-size:.6875rem; line-height:1.5; color:rgb(var(--theme-text-secondary)); } .more { padding:.5rem; text-align:left; } .error { color:rgb(var(--theme-error)); overflow-wrap:anywhere; } footer { border-top:1px solid rgb(var(--theme-border)/.35); padding-top:.6rem; }
  button:disabled { opacity:.5; cursor:default; } button:focus-visible { outline:2px solid rgb(var(--theme-link)); outline-offset:2px; }
</style>
