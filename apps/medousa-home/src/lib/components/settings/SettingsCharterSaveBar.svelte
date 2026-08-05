<script lang="ts">
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { workshopConfigOnHostHint } from "$lib/platformCopy";

  interface Props {
    mobile?: boolean;
    /**
     * Runs immediately before `workshopDefaults.save()` (e.g. flush textarea → draft).
     * Return `false` to abort save (e.g. user declined a confirm).
     */
    beforeSave?: () => boolean | void | Promise<boolean | void>;
    onSaved?: () => void | Promise<void>;
  }

  let { mobile = false, beforeSave, onSaved }: Props = $props();

  const mobileReadOnly = $derived(mobile && isTauriMobilePlatform());
  const dirty = $derived(workshopDefaults.dirty);
</script>

{#if workshopDefaults.loading}
  <p class="workshop-faint text-sm">Loading settings…</p>
{:else if mobileReadOnly}
  <p class="workshop-faint rounded-container-token border border-surface-500/35 bg-surface-900/40 px-3 py-2 text-xs leading-relaxed">
    {workshopConfigOnHostHint()} See
    <span class="font-mono text-surface-400">tui_defaults.json</span> in Workshop → Files & diagnostics.
  </p>
{:else}
  <div class="settings-save-bar">
    <button
      type="button"
      class="btn btn-sm variant-filled-primary"
      title="Save this section to the engine"
      disabled={workshopDefaults.saving || workshopDefaults.loading || !dirty}
      onclick={async () => {
        try {
          const proceed = await beforeSave?.();
          if (proceed === false) return;
          await workshopDefaults.save();
          if (workshopDefaults.message?.toLowerCase().includes("saved")) {
            await onSaved?.();
          }
        } catch (err) {
          workshopDefaults.message =
            err instanceof Error ? err.message : String(err);
        }
      }}
    >
      {workshopDefaults.saving ? "Saving…" : "Save"}
    </button>
    {#if dirty && !workshopDefaults.saving}
      <span class="settings-save-dirty">Unsaved changes</span>
    {/if}
    {#if workshopDefaults.message}
      <p
        class="text-xs {workshopDefaults.message.toLowerCase().includes('saved')
          ? 'text-success-400'
          : 'text-warning-400'}"
      >
        {workshopDefaults.message}
      </p>
    {/if}
  </div>
{/if}
