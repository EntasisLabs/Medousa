<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";

  let captureLine = $state("");

  async function handleCreateDaily() {
    await vault.createDailyNote();
  }

  async function handleQuickCapture() {
    if (!captureLine.trim()) return;
    await vault.quickCapture(captureLine);
    captureLine = "";
  }

  async function handleOpenLast() {
    const path = vault.lastNotePath;
    if (path) await vault.openNote(path);
  }
</script>

<div class="flex flex-1 flex-col items-center justify-center gap-6 p-8 text-center">
  <div class="max-w-md space-y-2">
    <h2 class="text-lg font-semibold text-surface-50">Your garage</h2>
    <p class="text-sm text-surface-400">
      Notes, journals, and the messy parts of life — organized by space, edited by you.
    </p>
  </div>

  <div class="flex flex-wrap items-center justify-center gap-2">
    <button
      type="button"
      class="btn variant-filled-primary"
      onclick={handleCreateDaily}
      disabled={vault.saving}
    >
      New daily note
    </button>
    <button
      type="button"
      class="btn variant-soft-surface"
      onclick={() => vault.openNewNoteDialog()}
    >
      New note…
    </button>
    {#if vault.lastNotePath}
      <button type="button" class="btn variant-ghost-surface" onclick={handleOpenLast}>
        Open last note
      </button>
    {/if}
  </div>

  <form
    class="flex w-full max-w-md gap-2"
    onsubmit={(event) => {
      event.preventDefault();
      void handleQuickCapture();
    }}
  >
    <input
      class="input flex-1 text-sm"
      type="text"
      placeholder="Quick capture to Inbox…"
      bind:value={captureLine}
    />
    <button
      type="submit"
      class="btn variant-soft-primary"
      disabled={!captureLine.trim() || vault.saving}
    >
      Capture
    </button>
  </form>
</div>
