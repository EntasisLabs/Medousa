<script lang="ts">
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const mobileReadOnly = $derived(mobile && isTauriMobilePlatform());
</script>

{#if workshopDefaults.loading}
  <p class="workshop-faint text-sm">Loading your charter…</p>
{:else if mobileReadOnly}
  <p class="workshop-faint rounded-container-token border border-surface-500/35 bg-surface-900/40 px-3 py-2 text-xs leading-relaxed">
    These values live on your Mac workshop. Change Memory and Voice on the Mac, or edit
    <span class="font-mono text-surface-400">tui_defaults.json</span> in Basement → Workshop files.
  </p>
{:else}
  <div class="flex flex-wrap items-center gap-3">
    <button
      type="button"
      class="btn btn-sm variant-filled-primary"
      disabled={workshopDefaults.saving || workshopDefaults.loading}
      onclick={() => workshopDefaults.save()}
    >
      {workshopDefaults.saving ? "Saving…" : "Save charter"}
    </button>
    {#if workshopDefaults.message}
      <p
        class="text-xs {workshopDefaults.message.includes('saved')
          ? 'text-success-400'
          : 'text-warning-400'}"
      >
        {workshopDefaults.message}
      </p>
    {/if}
  </div>
{/if}
