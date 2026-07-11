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
    type VaultUserTemplate,
  } from "$lib/utils/vaultUserTemplates";
  import { tick } from "svelte";
  import { X } from "@lucide/svelte";

  type Picker = null | "space" | "template";

  const SPACE_PREVIEW = 4;

  let spaceId = $state("journal");
  let templateId = $state<VaultTemplateId>("daily");
  let userTemplateId = $state("");
  let title = $state("");
  let wasOpen = $state(false);
  let userTemplates = $state<VaultUserTemplate[]>([]);
  let manageTemplates = $state(false);
  let picker = $state<Picker>(null);
  let showAllSpaces = $state(false);

  const templateOptions = $derived(templatesForSpace(spaceId));
  const creatableSpaces = $derived(creatableVaultSpaces());
  const usingUserTemplate = $derived(Boolean(userTemplateId));
  const spaceLabel = $derived(
    creatableSpaces.find((space) => space.id === spaceId)?.label ?? "Space",
  );
  const templateLabel = $derived.by(() => {
    if (userTemplateId) {
      return (
        userTemplates.find((row) => row.id === userTemplateId)?.name ?? "Template"
      );
    }
    return (
      templateOptions.find((option) => option.id === templateId)?.label ??
      "Template"
    );
  });
  const titleRequired = $derived(
    !usingUserTemplate && templateId !== "daily" && templateId !== "weekly",
  );
  const visibleSpaces = $derived.by(() => {
    if (showAllSpaces || creatableSpaces.length <= SPACE_PREVIEW) {
      return creatableSpaces;
    }
    const selected = creatableSpaces.find((space) => space.id === spaceId);
    const rest = creatableSpaces.filter((space) => space.id !== spaceId);
    const head = selected ? [selected, ...rest] : rest;
    return head.slice(0, SPACE_PREVIEW);
  });
  const hiddenSpaceCount = $derived(
    Math.max(0, creatableSpaces.length - visibleSpaces.length),
  );

  $effect(() => {
    const open = vault.newNoteDialogOpen;
    if (open && !wasOpen) {
      spaceId = vault.defaultCreateSpaceId;
      templateId = defaultTemplateForSpace(spaceId);
      userTemplateId = "";
      title = vault.newNotePrefillTitle.trim();
      userTemplates = listVaultUserTemplates();
      manageTemplates = false;
      picker = null;
      showAllSpaces = false;
      void tick().then(() => {
        const el = document.querySelector(
          "[data-new-note-title]",
        ) as HTMLInputElement | null;
        el?.focus();
        if (title) el?.select();
      });
    }
    wasOpen = open;
  });

  function togglePicker(next: Picker) {
    picker = picker === next ? null : next;
    if (picker !== "space") showAllSpaces = false;
    if (picker !== "template") manageTemplates = false;
  }

  function selectSpace(id: string) {
    spaceId = id;
    templateId = defaultTemplateForSpace(spaceId);
    userTemplateId = "";
    picker = null;
    showAllSpaces = false;
  }

  function selectBuiltInTemplate(id: VaultTemplateId) {
    templateId = id;
    userTemplateId = "";
    picker = null;
  }

  function selectUserTemplate(id: string) {
    userTemplateId = id;
    const selected = userTemplates.find((row) => row.id === id);
    if (selected?.spaceId) {
      spaceId = selected.spaceId;
    }
    picker = null;
  }

  function handleDeleteUserTemplate(id: string) {
    deleteVaultUserTemplate(id);
    userTemplates = listVaultUserTemplates();
    if (userTemplateId === id) {
      userTemplateId = "";
    }
  }

  async function handleCreate(event: Event) {
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

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (picker) {
        picker = null;
        return;
      }
      vault.closeNewNoteDialog();
    }
  }
</script>

{#if vault.newNoteDialogOpen}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="new-note-title"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) vault.closeNewNoteDialog();
    }}
  >
    <form
      class="vault-interact-sheet vault-compose-sheet"
      onsubmit={handleCreate}
    >
      <header class="vault-interact-header vault-compose-header">
        <h3 id="new-note-title" class="sr-only">
          {vault.newNotePrefillTitle ? "Create linked note" : "New note"}
        </h3>
        <button
          type="button"
          class="vault-interact-dismiss ml-auto"
          aria-label="Close"
          onclick={() => vault.closeNewNoteDialog()}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      {#if vault.newNotePrefillPath}
        <p class="vault-interact-note">
          For
          <span class="font-mono text-surface-300">{vault.newNotePrefillPath}</span>
        </p>
      {/if}

      <input
        class="vault-compose-title"
        type="text"
        placeholder="What are you writing?"
        bind:value={title}
        data-new-note-title
        required={titleRequired}
      />

      <div class="vault-compose-meta">
        <p class="vault-compose-sentence">
          in
          <button
            type="button"
            class="vault-compose-em-btn"
            class:vault-compose-em-btn--open={picker === "space"}
            aria-expanded={picker === "space"}
            onclick={() => togglePicker("space")}
          >
            {spaceLabel}
          </button>
          · starting from
          <button
            type="button"
            class="vault-compose-em-btn"
            class:vault-compose-em-btn--open={picker === "template"}
            aria-expanded={picker === "template"}
            onclick={() => togglePicker("template")}
          >
            {templateLabel}
          </button>
        </p>

        {#if picker === "space"}
          <div class="vault-chip-row" role="listbox" aria-label="Space">
            {#each visibleSpaces as space (space.id)}
              <button
                type="button"
                class="vault-chip"
                class:vault-chip--active={spaceId === space.id}
                role="option"
                aria-selected={spaceId === space.id}
                onclick={() => selectSpace(space.id)}
              >
                {space.label}
              </button>
            {/each}
            {#if hiddenSpaceCount > 0}
              <button
                type="button"
                class="vault-chip vault-chip--more"
                onclick={() => (showAllSpaces = true)}
              >
                More…
              </button>
            {/if}
          </div>
        {:else if picker === "template"}
          <div class="vault-chip-row" role="listbox" aria-label="Template">
            {#each templateOptions as option (option.id)}
              <button
                type="button"
                class="vault-chip"
                class:vault-chip--active={!usingUserTemplate && templateId === option.id}
                role="option"
                aria-selected={!usingUserTemplate && templateId === option.id}
                onclick={() => selectBuiltInTemplate(option.id)}
              >
                {option.label}
              </button>
            {/each}
            {#each userTemplates as template (template.id)}
              <button
                type="button"
                class="vault-chip vault-chip--yours"
                class:vault-chip--active={userTemplateId === template.id}
                role="option"
                aria-selected={userTemplateId === template.id}
                onclick={() => selectUserTemplate(template.id)}
              >
                {template.name}
              </button>
            {/each}
          </div>
          {#if userTemplates.length > 0}
            <button
              type="button"
              class="vault-compose-manage"
              onclick={() => (manageTemplates = !manageTemplates)}
            >
              {manageTemplates ? "Hide templates" : "Manage your templates"}
            </button>
            {#if manageTemplates}
              <ul class="vault-compose-manage-list">
                {#each userTemplates as template (template.id)}
                  <li>
                    <span class="truncate">{template.name}</span>
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
          {/if}
        {/if}
      </div>

      <div class="vault-compose-footer">
        <button
          type="submit"
          class="vault-interact-commit"
          disabled={vault.saving}
        >
          Create
        </button>
      </div>
    </form>
  </div>
{/if}
