<script lang="ts">
  import { tick } from "svelte";
  import { PenTool, Pin, PinOff, Pencil, Trash2, Ellipsis, Check, X } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { editCreativeProject, deleteCreativeProject } from "$lib/create/creativeProjectActions";
  import type { CreativeProjectSummary } from "$lib/types/generated/daemon_api";
  let { project, scope, current = false, onopen, onfocus }: {
    project: CreativeProjectSummary; scope: string; current?: boolean; onopen: () => void; onfocus: () => void;
  } = $props();
  let menu = $state(false), renaming = $state(false), confirming = $state(false), busy = $state(false);
  let draft = $state(""), error = $state<string | null>(null);
  let input = $state<HTMLInputElement | null>(null), openButton = $state<HTMLButtonElement | null>(null);
  let cancelDelete = $state<HTMLButtonElement | null>(null);
  async function rename() {
    menu = false; confirming = false; error = null;
    draft = project.title; renaming = true;
    await tick(); input?.focus(); input?.select();
  }
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true; error = null; menu = false;
    try { await action(); renaming = false; confirming = false; onfocus(); }
    catch (e) { error = e instanceof Error ? e.message : String(e); }
    finally { busy = false; }
  }
  function saveName(event: SubmitEvent) {
    event.preventDefault();
    const title = draft.trim();
    if (!title || busy) return;
    if (title === project.title) { renaming = false; onfocus(); return; }
    void run(() => editCreativeProject(scope, project.id, { op: "rename", title }));
  }
  async function confirmDelete() {
    menu = false; renaming = false; error = null; confirming = true;
    await tick(); cancelDelete?.focus();
  }
  function key(event: KeyboardEvent) {
    if (busy) return;
    if (event.key === "F2") { event.preventDefault(); event.stopPropagation(); void rename(); }
    if (event.key === "Delete" || event.key === "Backspace") { event.preventDefault(); event.stopPropagation(); confirmDelete(); }
    if (event.key === "F10" && event.shiftKey) { event.preventDefault(); event.stopPropagation(); menu = true; }
  }
  function cancel() { if (!busy) { renaming = false; confirming = false; error = null; void tick().then(() => openButton?.focus()); } }
</script>

<div class="create-rail-project" class:current class:menu-open={menu} data-project-id={project.id}>
  {#if renaming}
    <form class="create-rail-rename" onsubmit={saveName}>
      <PenTool size={14} strokeWidth={1.6}/>
      <input bind:this={input} bind:value={draft} aria-label="Project name" maxlength="256" disabled={busy}
        onkeydown={(event) => { event.stopPropagation(); if (event.key === "Escape") { event.preventDefault(); cancel(); } }}/>
      <button type="submit" class="create-rail-mini" aria-label="Save project name" disabled={busy || !draft.trim()}><Check size={13}/></button>
      <button type="button" class="create-rail-mini" aria-label="Cancel rename" disabled={busy} onclick={cancel}><X size={13}/></button>
    </form>
  {:else}
    <button bind:this={openButton} type="button" class="create-rail-project-open" data-rail-nav data-project-open
      aria-current={current ? "page" : undefined} title={project.title} onclick={onopen} onkeydown={key}
      oncontextmenu={(event) => { event.preventDefault(); menu = true; }}>
      <PenTool size={14} strokeWidth={1.6}/><span>{project.title}</span>
    </button>
    {#if project.pinned}<Pin size={11} class="create-rail-pin" aria-label="Pinned"/>{/if}
    <OverflowMenu bind:open={menu} label={`Actions for ${project.title}`} panelClass="vault-dock-popover create-rail-menu" panelWidth={190}
      onOpenChange={(open) => { if (!open && !renaming && !confirming && !busy) onfocus(); }}>
      {#snippet trigger({ open, toggle })}
        <button type="button" class="vault-dock-icon-btn create-rail-row-menu" aria-label={`Actions for ${project.title}`}
          aria-haspopup="menu" aria-expanded={open} title="Project actions" disabled={busy} onclick={toggle}><Ellipsis size={14}/></button>
      {/snippet}
      <button type="button" role="menuitem" onclick={() => void rename()}><Pencil size={14}/>Rename<span class="create-rail-shortcut">F2</span></button>
      <button type="button" role="menuitem" onclick={() => void run(() => editCreativeProject(scope, project.id, { op: "pin", pinned: !project.pinned }))}>
        {#if project.pinned}<PinOff size={14}/>{:else}<Pin size={14}/>{/if}{project.pinned ? "Unpin project" : "Pin project"}
      </button>
      <div class="vault-dock-popover__sep" role="separator"></div>
      <button type="button" role="menuitem" class="create-rail-danger" onclick={confirmDelete}><Trash2 size={14}/>Delete project…</button>
    </OverflowMenu>
  {/if}
</div>
{#if confirming}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions (Escape cancels the inline confirmation.) -->
  <div class="create-rail-confirm" role="group" aria-label={`Delete ${project.title}`}
    onkeydown={(event) => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); cancel(); } }}>
    <p>Delete “{project.title}”? Its canvas and images will be removed.</p>
    <div><button bind:this={cancelDelete} type="button" disabled={busy} onclick={cancel}>Cancel</button><button type="button" class="create-rail-danger" disabled={busy}
      onclick={() => void run(() => deleteCreativeProject(scope, project.id))}>{busy ? "Deleting…" : "Delete project"}</button></div>
  </div>
{/if}
{#if error}<p class="create-rail-error" role="alert">{error}</p>{/if}
