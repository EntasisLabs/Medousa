<script lang="ts">
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";

  interface Props {
    /**
     * Runs immediately before `workshopDefaults.save()` (e.g. flush textarea → draft).
     * Return `false` to abort save (e.g. user declined a confirm).
     */
    beforeSave?: () => boolean | void | Promise<boolean | void>;
    onSaved?: () => void | Promise<void>;
  }

  let { beforeSave, onSaved }: Props = $props();
  const dirty = $derived(workshopDefaults.dirty);
</script>

{#if workshopDefaults.loading}
  <p class="workshop-faint text-sm">Loading settings…</p>
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
          ? 'text-content-success'
          : 'text-content-warning'}"
      >
        {workshopDefaults.message}
      </p>
    {/if}
  </div>
{/if}
