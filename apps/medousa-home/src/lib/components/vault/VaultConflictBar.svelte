<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";

  async function handleReload() {
    await vault.reloadFromServer();
  }

  async function handleKeepMine() {
    await vault.keepMineAndSave();
  }
</script>

{#if vault.saveStatus === "conflict"}
  <div
    class="vault-conflict-bar flex flex-wrap items-center justify-between gap-3 border-b border-warning-500/35 bg-warning-500/10 px-4 py-2"
    role="alert"
  >
    <p class="text-sm text-warning-200">
      {vault.conflictMessage ?? "This note changed elsewhere while you were editing."}
    </p>
    <div class="flex shrink-0 gap-2">
      <button
        type="button"
        class="btn btn-sm variant-soft-surface"
        disabled={vault.saving}
        onclick={handleReload}
      >
        Reload
      </button>
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={vault.saving}
        onclick={handleKeepMine}
      >
        Keep mine
      </button>
    </div>
  </div>
{/if}
