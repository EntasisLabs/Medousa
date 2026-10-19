<script lang="ts">
  import { FileText, ArrowUpRight } from "@lucide/svelte";
  import { contextNotes } from "$lib/context/contextNotes.svelte";
  import { contextThreads } from "$lib/context/contextThreads.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { contextNotesForSession } from "$lib/utils/contextBrowse";

  let { sessionId }: { sessionId: string } = $props();
  const notes = $derived(contextNotesForSession(contextNotes.notes, sessionId, contextThreads.nodes.map((node) => node.session_id)));
</script>

{#if notes.length > 0}
  <section class="mt-5 border-t border-surface-700/40 pt-4" aria-label="Related notes">
    <h3 class="text-[11px] uppercase tracking-wider text-content-quiet">Related notes</h3>
    <p class="mt-1 text-xs text-content-quiet">Linked to this source session · opens in Notes</p>
    <ul class="mt-2 space-y-1">
      {#each notes as note (note.path)}
        <li><button type="button" class="flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left hover:bg-surface-800/60" onclick={() => void lmeWorkspace.openNote(note.path)}>
          <FileText size={14} class="shrink-0 text-content-secondary" />
          <span class="min-w-0 flex-1"><span class="block truncate text-xs text-surface-100">{note.title || note.path}</span><span class="block truncate text-[11px] text-content-quiet">{note.path}</span></span>
          <ArrowUpRight size={13} class="shrink-0 text-content-quiet" />
        </button></li>
      {/each}
    </ul>
  </section>
{:else if contextNotes.error}
  <p class="mt-5 text-xs text-content-quiet">Related note links could not be refreshed.</p>
{/if}
