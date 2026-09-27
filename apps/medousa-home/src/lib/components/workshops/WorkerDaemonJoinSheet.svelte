<script lang="ts">
  import { Cpu, LoaderCircle } from "@lucide/svelte";
  import { pairWorkerDaemon } from "$lib/utils/workerDaemonApi";
  import { parsePairQrUrl } from "$lib/utils/pairingUrl";

  interface Props {
    open: boolean;
    onClose: () => void;
    onPaired?: () => void;
  }
  let { open, onClose, onPaired }: Props = $props();
  let pairLink = $state("");
  let workerUrl = $state("");
  let label = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    pairLink = "";
    workerUrl = "";
    label = "";
    error = null;
  });

  function inferDetails() {
    const parsed = parsePairQrUrl(pairLink.trim());
    if (parsed && !workerUrl.trim()) workerUrl = parsed.daemonUrl;
    if (parsed && !label.trim()) label = parsed.peerName;
  }

  async function submit() {
    if (!pairLink.trim()) return;
    busy = true;
    error = null;
    try {
      inferDetails();
      await pairWorkerDaemon(pairLink.trim(), {
        workerUrl: workerUrl.trim() || undefined,
        label: label.trim() || undefined,
      });
      onPaired?.();
      onClose();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
    <div class="card w-full max-w-lg space-y-4 p-5 shadow-xl" role="dialog" aria-label="Add worker daemon">
      <header class="flex items-start justify-between gap-4">
        <div>
          <h2 class="text-sm font-semibold text-surface-50">Add worker daemon</h2>
          <p class="workshop-faint mt-1 text-xs leading-relaxed">The local engine joins with its own identity. This does not add the worker as a Home portal.</p>
        </div>
        <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onClose}>Cancel</button>
      </header>
      <label class="block">
        <span class="workshop-label">Worker pairing link</span>
        <textarea class="textarea mt-1 min-h-[5rem] w-full font-mono text-xs" placeholder="medousa://pair/2.0?a=…" bind:value={pairLink} oninput={inferDetails}></textarea>
      </label>
      <label class="block">
        <span class="workshop-label">Worker address</span>
        <input class="input mt-1 w-full font-mono text-xs" placeholder="http://192.168.1.42:7419" bind:value={workerUrl} />
      </label>
      <label class="block">
        <span class="workshop-label">Name</span>
        <input class="input mt-1 w-full text-sm" placeholder="Studio Mac" bind:value={label} />
      </label>
      {#if error}<p class="text-sm text-content-error">{error}</p>{/if}
      <button type="button" class="btn variant-filled-primary w-full" disabled={busy || !pairLink.trim()} onclick={() => void submit()}>
        {#if busy}<LoaderCircle class="mr-2 h-4 w-4 animate-spin" /> Pairing…{:else}<Cpu class="mr-2 h-4 w-4" /> Add worker daemon{/if}
      </button>
    </div>
  </div>
{/if}
