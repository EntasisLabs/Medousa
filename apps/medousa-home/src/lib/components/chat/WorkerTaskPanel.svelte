<script lang="ts">
  import { X } from "@lucide/svelte";
  import { tick } from "svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { workerTranscripts } from "$lib/work/workerTranscripts.svelte";
  import { renderMarkdown } from "$lib/markdown";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";

  let { workId, onClose }: { workId: string | null; onClose: () => void } = $props();
  let dialog = $state<HTMLDialogElement>();
  let failed = $state(false);
  const task = $derived(workId ? workerTranscripts.transcriptFor(workId) : null);
  const answer = $derived(task?.resultText?.trim() || task?.output?.trim() || '');
  $effect(() => {
    const id = workId;
    failed = false;
    if (!id) return;
    let current = true;
    void workerTranscripts.refresh(id).then(result => { if (current && !result) failed = true; });
    void tick().then(() => { if (current) dialog?.showModal(); });
    return () => { current = false; };
  });
  $effect(() => {
    if (!workId) return;
    return registerMobileBackHandler(() => { onClose(); return true; });
  });
</script>

{#if workId}
  <BodyPortal>
    <dialog bind:this={dialog} aria-label="Agent task result" onclose={onClose} onclick={(event) => { if (event.target === dialog) dialog?.close(); }}>
      <header><span>Agent task</span><button type="button" aria-label="Close task result" onclick={onClose}><X size={18} /></button></header>
      <h2>{task?.title ?? 'Task result'}</h2>
      {#if !task}
        <p role="status">{failed ? 'This task could not be loaded. Close and reopen it to try again.' : 'Loading task…'}</p>
      {:else}
        <p class="status">{task.streaming ? 'Working' : task.terminal ? 'Session ended' : task.statusLine || 'Status unavailable'}</p>
        <section><h3>{task.streaming ? 'Latest output' : 'Result'}</h3>
          {#if answer}<div class="markdown-body">{@html renderMarkdown(answer)}</div>
          {:else}<p>{task.streaming ? 'The agent is working. Its result will appear here.' : 'No written result was recorded.'}</p>{/if}
        </section>
        {#if task.error}<p role="alert">{task.error}</p>{/if}
        <p class="scope">A returned result does not mean changes were approved, applied or published.</p>
      {/if}
    </dialog>
  </BodyPortal>
{/if}

<style>
  dialog { position: fixed; inset: 0 0 0 auto; margin: 0; width: min(520px, 100%); max-width: 100%; height: 100%; max-height: 100%; padding: 20px; overflow: auto; border: 1px solid rgb(var(--theme-border)); background: rgb(var(--theme-card)); color: rgb(var(--theme-text)); }
  dialog::backdrop { background: rgb(0 0 0 / .25); }
  header { display: flex; align-items: center; justify-content: space-between; color: rgb(var(--theme-text-secondary)); font-size: 12px; margin-bottom: 20px; }
  button { border: 0; background: transparent; color: inherit; padding: 6px; cursor: pointer; }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 3px; }
  h2 { font-size: 20px; overflow-wrap: anywhere; }
  h3 { font-size: 12px; color: rgb(var(--theme-text-secondary)); margin-bottom: 12px; }
  section { padding-block: 20px; margin-top: 18px; border-top: 1px solid rgb(var(--theme-border)); }
  p, .markdown-body { font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; }
  .status, .scope { color: rgb(var(--theme-text-secondary)); margin-block: 12px; }
  @media (pointer: coarse) { button { min-height: 44px; min-width: 44px; } }
</style>
