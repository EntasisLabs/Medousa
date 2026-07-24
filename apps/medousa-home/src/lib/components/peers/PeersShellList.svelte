<script lang="ts">
  import PeerListRow from "$lib/components/peers/PeerListRow.svelte";
  import { peersShell } from "$lib/stores/peersShell.svelte";
  import { Plus, Search, Users } from "@lucide/svelte";

  interface Props {
    /** Fired after a peer is chosen (e.g. open the peers surface from a rail popover). */
    onPickPeer?: (id: string) => void;
    /** `rail-list` hides header actions (they live in the rail popover strip). */
    chrome?: "default" | "rail-list";
  }

  let { onPickPeer, chrome = "default" }: Props = $props();

  const showHeader = $derived(chrome !== "rail-list");
</script>

<div class="peers-shell-list flex h-full min-h-0 flex-col">
  {#if showHeader}
    <div class="flex items-center justify-between gap-2 px-3 pb-2 pt-1">
      {#if peersShell.nearbyUntrustedCount > 0}
        <p class="workshop-faint text-[11px]">
          {peersShell.nearbyUntrustedCount} nearby
        </p>
      {:else}
        <span></span>
      {/if}
      <button
        type="button"
        class="inline-flex size-7 items-center justify-center rounded-md text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-100"
        title="Add peer"
        aria-label="Add peer"
        onclick={() => peersShell.requestAddPeer()}
      >
        <Plus size={15} strokeWidth={1.75} />
      </button>
    </div>

    {#if peersShell.showPeopleSearch}
      <label class="peers-sidebar-search mx-3 mb-2">
        <Search size={14} strokeWidth={1.75} class="peers-sidebar-search-icon" aria-hidden="true" />
        <input
          class="peers-sidebar-search-input"
          type="search"
          placeholder="Search people…"
          bind:value={peersShell.peopleQuery}
        />
      </label>
    {/if}
  {:else if peersShell.nearbyUntrustedCount > 0}
    <p class="workshop-faint px-3 pb-1 pt-1 text-[11px]">
      {peersShell.nearbyUntrustedCount} nearby
    </p>
  {/if}

  {#if !peersShell.hasPeers}
    <div class="peers-empty-people px-3">
      <Users size={22} strokeWidth={1.5} />
      <p>Add someone nearby</p>
      <button type="button" class="btn btn-sm btn-primary" onclick={() => peersShell.requestAddPeer()}>
        Add peer
      </button>
    </div>
  {:else if peersShell.rows.length === 0}
    <div class="peers-empty-people px-3">
      <p>No people match “{peersShell.peopleQuery.trim()}”.</p>
    </div>
  {:else}
    <ul class="peers-people peers-people-scroll min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
      {#each peersShell.rows as row (row.workshopId)}
        <li>
          <PeerListRow
            {row}
            selected={peersShell.selectedPeerId === row.workshopId}
            onSelect={() => {
              peersShell.selectPeer(row.workshopId);
              onPickPeer?.(row.workshopId);
            }}
          />
        </li>
      {/each}
    </ul>
  {/if}
</div>
