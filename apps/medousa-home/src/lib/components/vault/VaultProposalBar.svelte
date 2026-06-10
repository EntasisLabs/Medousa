<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { formatDiffChip } from "$lib/utils/vaultDiff";

  const diffChip = $derived(
    vault.proposalContent
      ? formatDiffChip(
          vault.proposalDiffStats() ?? { added: 0, removed: 0, changed: 0 },
        )
      : null,
  );

  async function handleAccept() {
    await vault.acceptProposal();
  }

  async function handleDiscard() {
    await vault.discardProposal();
  }

  function handleEdit() {
    vault.editProposal();
  }
</script>

{#if vault.proposalActive}
  <div
    class="vault-proposal-bar flex flex-wrap items-center justify-between gap-3 border-b border-primary-500/35 bg-primary-500/10 px-4 py-2"
    role="alert"
  >
    <p class="text-sm text-primary-100">
      {#if vault.proposalSource === "agent"}
        Agent updated this note
      {:else}
        This note changed elsewhere
      {/if}
      {#if diffChip}
        <span class="text-primary-200/80">· {diffChip}</span>
      {/if}
    </p>
    <div class="flex shrink-0 flex-wrap gap-2">
      <button
        type="button"
        class="btn btn-sm variant-soft-surface"
        disabled={vault.saving}
        onclick={handleEdit}
      >
        Keep editing
      </button>
      <button
        type="button"
        class="btn btn-sm variant-soft-surface"
        disabled={vault.saving}
        onclick={handleDiscard}
      >
        Take {vault.proposalSource === "agent" ? "agent" : "server"} version
      </button>
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={vault.saving}
        onclick={handleAccept}
      >
        Keep mine
      </button>
    </div>
  </div>
{/if}
