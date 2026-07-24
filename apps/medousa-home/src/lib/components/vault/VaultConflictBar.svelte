<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";

  async function handleReload() {
    await vault.reloadFromServer();
  }

  async function handleKeepMine() {
    await vault.keepMineAndSave();
  }

  const versionsAvailable = $derived(
    Boolean(workshopDefaults.draft.vaultGitEnabled && vaultVersions.status?.isRepo),
  );
</script>

{#if vault.saveStatus === "conflict"}
  <div
    class="vault-conflict-bar flex flex-wrap items-center justify-between gap-3 border-b border-warning-500/35 bg-warning-500/10 px-4 py-2"
    role="alert"
  >
    <p class="text-sm text-warning-200">
      {vault.conflictMessage ?? "This note changed elsewhere while you were editing."}
    </p>
    <div class="flex shrink-0 flex-wrap gap-2">
      {#if versionsAvailable}
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => vaultVersions.openPanel()}
        >
          History
        </button>
      {/if}
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
