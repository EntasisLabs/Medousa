<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";

  const pending = $derived(environment.pendingProposal);
  let dismissed = $state(false);

  $effect(() => {
    if (!pending) {
      dismissed = false;
    }
  });

  function reviewInSettings() {
    settingsNav.openSection("canvas");
    layout.navigateDesktop(SAFETY_SURFACE_SETTINGS, { bump: true });
  }
</script>

{#if pending && !dismissed}
  <div class="env-global-pending-banner" role="status">
    <p class="text-sm text-surface-100">
      Medousa proposed a layout change — Review in Settings → Canvas
    </p>
    <div class="mt-2 flex flex-wrap gap-2">
      <button type="button" class="btn btn-xs btn-primary" onclick={reviewInSettings}>
        Review
      </button>
      <button
        type="button"
        class="btn btn-xs btn-ghost"
        disabled={environment.pendingBusy}
        onclick={() => void environment.dismissPendingProposal()}
      >
        Dismiss
      </button>
      <button type="button" class="btn btn-xs btn-ghost" onclick={() => (dismissed = true)}>
        Hide
      </button>
    </div>
  </div>
{/if}

<style>
  .env-global-pending-banner {
    border-bottom: 1px solid color-mix(in srgb, var(--color-primary-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 10%, transparent);
    padding: 0.5rem 0.75rem;
  }
</style>
