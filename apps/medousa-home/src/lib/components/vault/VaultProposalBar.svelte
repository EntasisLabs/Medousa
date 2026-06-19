<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { formatDiffChip } from "$lib/utils/vaultDiff";
  import VaultProposalDiffPreview from "./VaultProposalDiffPreview.svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const diffChip = $derived(
    vault.proposalContent
      ? formatDiffChip(
          vault.proposalDiffStats() ?? { added: 0, removed: 0, changed: 0 },
        )
      : null,
  );

  const showDiffPreview = $derived(
    mobile &&
      vault.proposalContent &&
      vault.content !== vault.proposalContent,
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
    class="vault-proposal-bar border-b border-primary-500/35 bg-primary-500/10 px-4 py-2 {mobile
      ? 'space-y-3'
      : 'flex flex-wrap items-center justify-between gap-3'}"
    role="alert"
  >
    <div class="min-w-0">
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
      {#if mobile}
        <p class="mt-1 text-xs text-primary-200/70">
          Review changes before choosing a version.
        </p>
      {/if}
    </div>

    {#if showDiffPreview && vault.proposalContent}
      <VaultProposalDiffPreview
        before={vault.proposalContent}
        after={vault.content}
      />
    {/if}

    <div
      class="{mobile
        ? 'grid grid-cols-1 gap-2 sm:grid-cols-3'
        : 'flex shrink-0 flex-wrap gap-2'}"
    >
      <button
        type="button"
        class="btn btn-sm {mobile ? 'w-full' : ''} variant-soft-surface"
        disabled={vault.saving}
        onclick={handleEdit}
      >
        Keep editing
      </button>
      <button
        type="button"
        class="btn btn-sm {mobile ? 'w-full' : ''} variant-soft-surface"
        disabled={vault.saving}
        onclick={handleDiscard}
      >
        Take {vault.proposalSource === "agent" ? "agent" : "server"} version
      </button>
      <button
        type="button"
        class="btn btn-sm {mobile ? 'w-full' : ''} variant-filled-primary"
        disabled={vault.saving}
        onclick={handleAccept}
      >
        Keep mine
      </button>
    </div>
  </div>
{/if}
