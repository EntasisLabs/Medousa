<script lang="ts">
  import { untrack } from "svelte";
  import { contextShell } from "$lib/context/contextShell.svelte";
  import { contextThreads } from "$lib/context/contextThreads.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import ContextSavedMoment from "$lib/components/context/ContextSavedMoment.svelte";
  import ContextRelatedNotes from "$lib/components/context/ContextRelatedNotes.svelte";
  import { contextMemoryPresentation } from "$lib/utils/contextBrowse";
  import { formatContextWhen, sessionDisplayName } from "$lib/utils/contextHuman";

  let { sessionLabels }: { sessionLabels: Record<string, string> } = $props();
  const selected = $derived(contextShell.selectedMapNodeId);
  const selectedMoment = $derived(selected?.startsWith("thread:") ? selected.slice(7) : null);
  const selectedSession = $derived(selected?.startsWith("session:") ? selected.slice(8) : null);
  const detail = $derived(contextThreads.detail?.node.sync_key === selectedMoment ? contextThreads.detail : null);
  const knownChats = $derived(new Set(chat.sessions.map((session) => session.session_id)));
  const sessionMoments = $derived(contextThreads.nodes.filter((node) => node.session_id === selectedSession));

  $effect(() => {
    const key = selectedMoment;
    untrack(() => {
      if (key) void contextThreads.loadDetail(key);
      else contextThreads.clearDetail();
    });
  });

  async function openChat(sessionId: string) {
    shellTabs.openChat(sessionId, { activate: true });
    await chat.switchSession(sessionId);
  }
</script>

<section class="context-reader h-full overflow-y-auto" aria-label="Saved memory reader">
  <div class="context-reader-content">
    {#if selectedMoment}
      {#if contextThreads.detailError}
        <p class="text-sm text-content-warning">{contextThreads.detailError}</p>
        <button type="button" class="btn btn-sm variant-soft-surface mt-3" onclick={() => void contextThreads.loadDetail(selectedMoment!)}>Retry saved moment</button>
      {:else if detail}
        {#key detail.node.sync_key}
          <ContextSavedMoment {detail} {sessionLabels} />
        {/key}
      {:else}<p class="text-sm text-content-quiet">Loading saved memory…</p>{/if}
    {:else if selectedSession}
      <p class="text-[11px] uppercase tracking-wider text-content-quiet">Source session</p>
      <h2 class="mt-3 text-2xl font-medium text-surface-100">{sessionDisplayName(selectedSession, sessionLabels)}</h2>
      {#if knownChats.has(selectedSession)}<button type="button" class="btn btn-sm variant-soft-surface mt-4" onclick={() => void openChat(selectedSession!)}>Open source session</button>{/if}
      <h3 class="mb-2 mt-8 text-xs text-content-secondary">Saved memory from this session</h3>
      {#each sessionMoments as moment (moment.sync_key)}
        {@const presentation = contextMemoryPresentation(moment)}
        <button type="button" class="context-session-memory" onclick={() => contextShell.selectMapNode(`thread:${moment.sync_key}`)}>
          <span>{presentation.title}</span><span class="mt-1 block text-xs text-content-quiet">{presentation.kind} · {formatContextWhen(moment.timestamp)}</span>
        </button>
      {/each}
      <ContextRelatedNotes sessionId={selectedSession} />
      {#if !sessionMoments.length}<p class="text-sm text-content-quiet">No memory from this session is in the loaded context.</p>{/if}
      <details class="mt-6 text-xs text-content-quiet"><summary>Session details</summary><p class="mt-3 break-all font-mono">{selectedSession}</p></details>
    {:else}<p class="text-sm text-content-secondary">Choose a memory from the list to read it here.</p>{/if}
  </div>
</section>

<style>
  .context-reader-content { max-width: 50rem; margin-inline: auto; padding: clamp(1.5rem, 4vw, 3.5rem); overflow-wrap: anywhere; }
  .context-session-memory { display: block; width: 100%; text-align: left; padding: .85rem 0; border-bottom: 1px solid rgb(var(--color-surface-700) / .3); color: rgb(var(--color-surface-100)); font-size: 14px; }
</style>
