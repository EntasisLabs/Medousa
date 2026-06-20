<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { creatableVaultSpaces } from "$lib/config/vaultSpaces";
  import {
    templatesForSpace,
    defaultTemplateForSpace,
    resolveTemplateForSpace,
    type VaultTemplateId,
  } from "$lib/utils/vaultTemplates";
  import {
    deleteVaultUserTemplate,
    listVaultUserTemplates,
    saveVaultUserTemplate,
    type VaultUserTemplate,
  } from "$lib/utils/vaultUserTemplates";

  let spaceId = $state("journal");
  let templateId = $state<VaultTemplateId>("daily");
  let userTemplateId = $state("");
  let title = $state("");
  let wasOpen = $state(false);
  let userTemplates = $state<VaultUserTemplate[]>([]);
  let saveTemplateName = $state("");
  let templateMessage = $state<string | null>(null);

  const templateOptions = $derived(templatesForSpace(spaceId));
  const creatableSpaces = $derived(creatableVaultSpaces());
  const usingUserTemplate = $derived(Boolean(userTemplateId));

  $effect(() => {
    const open = vault.newNoteDialogOpen;
    if (open && !wasOpen) {
      spaceId = vault.defaultCreateSpaceId;
      templateId = defaultTemplateForSpace(spaceId);
      userTemplateId = "";
      title = vault.newNotePrefillTitle.trim();
      userTemplates = listVaultUserTemplates();
      saveTemplateName = "";
      templateMessage = null;
    }
    wasOpen = open;
  });

  function handleSpaceChange() {
    templateId = defaultTemplateForSpace(spaceId);
    userTemplateId = "";
  }

  function handleBuiltInTemplateChange() {
    userTemplateId = "";
  }

  function handleUserTemplateChange() {
    if (!userTemplateId) return;
    const selected = userTemplates.find((row) => row.id === userTemplateId);
    if (selected?.spaceId) {
      spaceId = selected.spaceId;
    }
  }

  async function handleCreateCustom(event: Event) {
    event.preventDefault();
    if (userTemplateId) {
      const selected = userTemplates.find((row) => row.id === userTemplateId);
      if (!selected) return;
      await vault.createNote({
        spaceId,
        title: title.trim() || selected.name,
        content: selected.content,
      });
    } else {
      const resolvedTemplate = resolveTemplateForSpace(spaceId, templateId);
      if (!title.trim() && resolvedTemplate !== "daily" && resolvedTemplate !== "weekly") {
        return;
      }
      await vault.createNote({
        spaceId,
        title: title.trim() || "Note",
        templateId: resolvedTemplate,
      });
    }
    title = "";
    userTemplateId = "";
    vault.closeNewNoteDialog();
  }

  function handleSaveCurrentTemplate() {
    templateMessage = null;
    if (!vault.selectedPath || !vault.content.trim()) {
      templateMessage = "Open a note first.";
      return;
    }
    const name = saveTemplateName.trim() || vault.title.trim() || "Saved template";
    const saved = saveVaultUserTemplate({
      name,
      content: vault.content,
      spaceId: vault.activeSpace?.id,
    });
    if (!saved) {
      templateMessage = "Could not save template.";
      return;
    }
    userTemplates = listVaultUserTemplates();
    userTemplateId = saved.id;
    saveTemplateName = "";
    templateMessage = `Saved “${saved.name}”.`;
  }

  function handleDeleteUserTemplate(id: string) {
    deleteVaultUserTemplate(id);
    userTemplates = listVaultUserTemplates();
    if (userTemplateId === id) {
      userTemplateId = "";
    }
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
          <select
            class="select w-full"
            bind:value={templateId}
            disabled={usingUserTemplate}
            onchange={handleBuiltInTemplateChange}
          >
            {#each templateOptions as option (option.id)}
              <option value={option.id}>{option.label}</option>
            {/each}
          </select>
        {/key}
      </label>

      {#if userTemplates.length > 0}
        <label class="block space-y-1 text-left text-sm">
          <span class="text-surface-400">Your templates</span>
          <select
            class="select w-full"
            bind:value={userTemplateId}
            onchange={handleUserTemplateChange}
          >
            <option value="">None</option>
            {#each userTemplates as template (template.id)}
              <option value={template.id}>{template.name}</option>
            {/each}
          </select>
        </label>
        <ul class="space-y-1 rounded-container-token border border-surface-500/35 p-2">
          {#each userTemplates as template (template.id)}
            <li class="flex items-center justify-between gap-2 text-xs">
              <span class="truncate text-surface-200">{template.name}</span>
              <button
                type="button"
                class="workshop-text-action shrink-0"
                onclick={() => handleDeleteUserTemplate(template.id)}
              >
                Delete
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <label class="block space-y-1 text-left text-sm">
        <span class="text-surface-400">Title</span>
        <input
          class="input w-full"
          type="text"
          placeholder="Weekly review, project plan…"
          bind:value={title}
          required={!usingUserTemplate && templateId !== "daily" && templateId !== "weekly"}
        />
      </label>

      <div class="rounded-container-token border border-surface-500/35 p-3">
        <p class="text-xs font-medium text-surface-300">Save current note as template</p>
        <div class="mt-2 flex gap-2">
          <input
            class="input min-w-0 flex-1 text-sm"
            type="text"
            placeholder={vault.title || "Template name"}
            bind:value={saveTemplateName}
          />
          <button
            type="button"
            class="btn btn-sm variant-soft-primary shrink-0"
            disabled={!vault.selectedPath}
            onclick={handleSaveCurrentTemplate}
          >
            Save
          </button>
        </div>
        {#if templateMessage}
          <p class="workshop-faint mt-2 text-xs">{templateMessage}</p>
        {/if}
      </div>

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
