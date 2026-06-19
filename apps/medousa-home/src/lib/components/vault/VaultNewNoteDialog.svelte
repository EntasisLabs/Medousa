<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { creatableVaultSpaces } from "$lib/config/vaultSpaces";
  import {
    templatesForSpace,
    defaultTemplateForSpace,
    resolveTemplateForSpace,
    type VaultTemplateId,
  } from "$lib/utils/vaultTemplates";

  let spaceId = $state("journal");
  let templateId = $state<VaultTemplateId>("daily");
  let title = $state("");
  let wasOpen = $state(false);

  const templateOptions = $derived(templatesForSpace(spaceId));
  const creatableSpaces = $derived(creatableVaultSpaces());

  $effect(() => {
    const open = vault.newNoteDialogOpen;
    if (open && !wasOpen) {
      spaceId = vault.defaultCreateSpaceId;
      templateId = defaultTemplateForSpace(spaceId);
      title = vault.newNotePrefillTitle.trim();
    }
    wasOpen = open;
  });

  function handleSpaceChange() {
    templateId = defaultTemplateForSpace(spaceId);
  }

  async function handleCreateCustom(event: Event) {
    event.preventDefault();
    const resolvedTemplate = resolveTemplateForSpace(spaceId, templateId);
    if (!title.trim() && resolvedTemplate !== "daily" && resolvedTemplate !== "weekly") {
      return;
    }
    await vault.createNote({
      spaceId,
      title: title.trim() || "Note",
      templateId: resolvedTemplate,
    });
    title = "";
    vault.closeNewNoteDialog();
  }
</script>

{#if vault.newNoteDialogOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="new-note-title"
  >
    <form
      class="card w-full max-w-md space-y-4 p-5 shadow-xl"
      onsubmit={handleCreateCustom}
    >
      <h3 id="new-note-title" class="text-base font-semibold">
        {vault.newNotePrefillTitle ? "Create linked note" : "New note"}
      </h3>
      {#if vault.newNotePrefillPath}
        <p class="text-xs text-surface-400">
          For wikilink
          <span class="font-mono text-surface-300">{vault.newNotePrefillPath}</span>
        </p>
      {/if}

      <label class="block space-y-1 text-left text-sm">
        <span class="text-surface-400">Space</span>
        <select
          class="select w-full"
          bind:value={spaceId}
          onchange={handleSpaceChange}
        >
          {#each creatableSpaces as space (space.id)}
            <option value={space.id}>{space.label}</option>
          {/each}
        </select>
      </label>

      <label class="block space-y-1 text-left text-sm">
        <span class="text-surface-400">Template</span>
        {#key spaceId}
          <select class="select w-full" bind:value={templateId}>
            {#each templateOptions as option (option.id)}
              <option value={option.id}>{option.label}</option>
            {/each}
          </select>
        {/key}
      </label>

      <label class="block space-y-1 text-left text-sm">
        <span class="text-surface-400">Title</span>
        <input
          class="input w-full"
          type="text"
          placeholder="Weekly review, project plan…"
          bind:value={title}
          required={templateId !== "daily" && templateId !== "weekly"}
        />
      </label>

      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="btn variant-ghost-surface"
          onclick={() => vault.closeNewNoteDialog()}
        >
          Cancel
        </button>
        <button
          type="submit"
          class="btn variant-filled-primary"
          disabled={vault.saving}
        >
          Create
        </button>
      </div>
    </form>
  </div>
{/if}
