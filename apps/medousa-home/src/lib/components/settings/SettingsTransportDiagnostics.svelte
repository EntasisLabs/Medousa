<script lang="ts">
  import { getWorkshopTransportDiagnostics, type TransportDiagnostic } from "$lib/daemon/client";
  import { copyTextToClipboard } from "$lib/utils/vaultClipboard";

  let events = $state<TransportDiagnostic[]>([]);
  let loaded = $state(false);
  let busy = $state(false);
  let message = $state("");

  async function load() {
    if (busy) return;
    busy = true;
    message = "";
    try {
      events = await getWorkshopTransportDiagnostics();
      loaded = true;
    } catch {
      message = "Could not read connection diagnostics.";
    } finally {
      busy = false;
    }
  }

  async function copy() {
    const copied = await copyTextToClipboard(JSON.stringify(events, null, 2));
    message = copied ? "Copied diagnostics." : "Could not copy diagnostics.";
  }
</script>

<details class="rounded-xl border border-surface-800 p-3" ontoggle={(event) => {
  if (event.currentTarget.open) void load();
}}>
  <summary class="cursor-pointer text-sm text-content-secondary">Connection diagnostics</summary>
  <p class="mt-3 text-xs text-content-quiet">Recent Iroh activity on this device. Refresh reads local records without contacting the workshop.</p>
  <div class="my-3 flex items-center gap-4">
    <button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void load()}>{busy ? "Loading…" : "Refresh"}</button>
    <button type="button" class="text-xs text-content-secondary" disabled={!loaded || busy} onclick={() => void copy()}>Copy diagnostics</button>
  </div>
  {#if message}<p class="mb-2 text-xs text-content-quiet" role="status">{message}</p>{/if}
  {#if loaded && events.length === 0}
    <p class="text-xs text-content-quiet">No Iroh activity recorded yet. LAN connections do not appear here.</p>
  {:else}
    <ul class="max-h-64 space-y-2 overflow-y-auto text-xs text-content-secondary">
      {#each events.slice(-16).reverse() as event (event.sequence)}
        <li class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
          <span class="text-content-quiet">#{event.sequence}</span>
          <span>{event.phase.replaceAll("_", " ")} · {event.outcome}</span>
          <span class="text-content-quiet">{event.elapsed_ms} ms{#if event.path} · {event.path}{/if}{#if event.rtt_ms !== null} · RTT {event.rtt_ms} ms{/if}</span>
        </li>
      {/each}
    </ul>
  {/if}
</details>
