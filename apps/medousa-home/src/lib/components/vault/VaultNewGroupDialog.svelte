<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";

  let label = $state("");

  $effect(() => {
    if (vault.newGroupDialogOpen) {
      label = "";
    }
  });

  async function handleCreate(event: Event) {
    event.preventDefault();
    const trimmed = label.trim();
    if (!trimmed) return;
    vault.addCustomGroup(trimmed);
    label = "";
    vault.closeNewGroupDialog();
  }
</script>

{#if vault.newGroupDialogOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="new-group-title"
  >
    <form
      class="card w-full max-w-md space-y-4 p-5 shadow-xl"
      onsubmit={(event) => void handleCreate(event)}
    >
      <h3 id="new-group-title" class="text-base font-semibold">New group</h3>
      <p class="text-sm text-surface-400">
        Create a top-level folder for your notes. Notes can be dragged into it from anywhere in the
        vault.
      </p>

      <label class="block space-y-1 text-left text-sm">
        <span class="text-surface-400">Group name</span>
        <input
          class="input w-full"
          type="text"
          placeholder="Research, Recipes, Clients…"
          bind:value={label}
          required
        />
      </label>

      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="btn variant-ghost-surface"
          onclick={() => vault.closeNewGroupDialog()}
        >
          Cancel
        </button>
        <button type="submit" class="btn variant-filled-primary" disabled={!label.trim()}>
          Create group
        </button>
      </div>
    </form>
  </div>
{/if}
