<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { creatableVaultSpaces, resolveSpaceForPath } from "$lib/config/vaultSpaces";
  import {
    allVaultTemplates,
    joinVaultFolder,
    type VaultTemplateId,
  } from "$lib/utils/vaultTemplates";
  import {
    deleteVaultUserTemplate,
    listVaultUserTemplates,
    type VaultUserTemplate,
  } from "$lib/utils/vaultUserTemplates";
  import { tick } from "svelte";
  import { X } from "@lucide/svelte";

  type Picker = null | "location" | "template";
  type LocationMode = "here" | "space";

  const SPACE_PREVIEW = 4;

  let locationMode = $state<LocationMode>("space");
  let spaceId = $state("journal");
  let herePrefix = $state<string | null>(null);
  let subfolder = $state("");
  let showSubfolder = $state(false);
  let templateId = $state<VaultTemplateId>("blank");
  let userTemplateId = $state("");
  let title = $state("");
  let wasOpen = $state(false);
  let userTemplates = $state<VaultUserTemplate[]>([]);
  let manageTemplates = $state(false);
  let picker = $state<Picker>(null);
  let showAllSpaces = $state(false);

  const templateOptions = $derived(allVaultTemplates());
  const creatableSpaces = $derived(creatableVaultSpaces());
  const usingUserTemplate = $derived(Boolean(userTemplateId));
  const canCreateHere = $derived(herePrefix !== null);

  const locationLabel = $derived.by(() => {
    if (locationMode === "here" && herePrefix !== null) {
      const base = formatFolderLabel(herePrefix);
      if (subfolder.trim()) {
        return `${base === "Vault root" ? "" : `${base}/`}${subfolder.trim()}`.replace(
          /^\/+/,
          "",
        );
      }
      return base;
    }
    const spaceLabel =
      creatableSpaces.find((space) => space.id === spaceId)?.label ?? "Space";
    if (subfolder.trim()) return `${spaceLabel} / ${subfolder.trim()}`;
    return spaceLabel;
  });

  const templateLabel = $derived.by(() => {
    if (userTemplateId) {
      return (
        userTemplates.find((row) => row.id === userTemplateId)?.name ?? "Template"
      );
    }
    return (
      templateOptions.find((option) => option.id === templateId)?.label ?? "Blank note"
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

  function formatFolderLabel(prefix: string): string {
    const trimmed = prefix.replace(/\/+$/, "");
    if (!trimmed) return "Vault root";
    const parts = trimmed.split("/");
    if (parts.length <= 2) return trimmed;
    return `…/${parts.slice(-2).join("/")}`;
  }

  function inferSpaceIdFromFolder(prefix: string): string {
    const probe = prefix ? `${prefix}note.md` : "note.md";
    const space = resolveSpaceForPath(probe, "note");
    if (space.id === "system_bucket") return vault.defaultCreateSpaceId;
    return space.id;
  }

  $effect(() => {
    const open = vault.newNoteDialogOpen;
    if (open && !wasOpen) {
      const folder = vault.currentCreateFolderPrefix;
      herePrefix = folder;
      subfolder = "";
      showSubfolder = false;
      if (folder !== null) {
        locationMode = "here";
        spaceId = inferSpaceIdFromFolder(folder);
      } else {
        locationMode = "space";
        spaceId = vault.defaultCreateSpaceId;
      }
      templateId = "blank";
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
    if (picker !== "location") showAllSpaces = false;
    if (picker !== "template") manageTemplates = false;
  }

  function selectHere() {
    if (herePrefix === null) return;
    locationMode = "here";
    spaceId = inferSpaceIdFromFolder(herePrefix);
    picker = null;
    showAllSpaces = false;
  }

  function selectSpace(id: string) {
    locationMode = "space";
    spaceId = id;
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
      locationMode = "space";
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
    const prefillPath = vault.newNotePrefillPath?.trim() || undefined;

    let createdPath: string | null = null;
    if (userTemplateId) {
      const selected = userTemplates.find((row) => row.id === userTemplateId);
      if (!selected) return;
      createdPath = await vault.createNote({
        spaceId,
        title: title.trim() || selected.name,
        content: selected.content,
        path: prefillPath,
        folderPrefix: locationMode === "here" ? (herePrefix ?? "") : undefined,
        subfolder: subfolder.trim() || undefined,
      });
    } else {
      if (!title.trim() && templateId !== "daily" && templateId !== "weekly") {
        return;
      }
      createdPath = await vault.createNote({
        spaceId,
        title: title.trim() || "Note",
        templateId,
        path: prefillPath,
        folderPrefix: locationMode === "here" ? (herePrefix ?? "") : undefined,
        subfolder: subfolder.trim() || undefined,
      });
    }
    // Keep the sheet open on collision / failure so the user can rename.
    if (!createdPath) return;
    title = "";
    userTemplateId = "";
    subfolder = "";
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
    tabindex="-1"
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
            class:vault-compose-em-btn--open={picker === "location"}
            aria-expanded={picker === "location"}
            onclick={() => togglePicker("location")}
          >
            {locationLabel}
          </button>
          · as
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

        {#if picker === "location"}
          <div class="vault-chip-row" role="listbox" aria-label="Location">
            {#if canCreateHere}
              <button
                type="button"
                class="vault-chip"
                class:vault-chip--active={locationMode === "here"}
                role="option"
                aria-selected={locationMode === "here"}
                onclick={selectHere}
                title={herePrefix ? joinVaultFolder(herePrefix) || "/" : "/"}
              >
                This folder
              </button>
            {/if}
            {#each visibleSpaces as space (space.id)}
              <button
                type="button"
                class="vault-chip"
                class:vault-chip--active={locationMode === "space" && spaceId === space.id}
                role="option"
                aria-selected={locationMode === "space" && spaceId === space.id}
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
          <button
            type="button"
            class="vault-compose-manage"
            onclick={() => (showSubfolder = !showSubfolder)}
          >
            {showSubfolder ? "Hide new subfolder" : "New subfolder…"}
          </button>
          {#if showSubfolder}
            <input
              class="vault-compose-subfolder"
              type="text"
              placeholder="Subfolder name"
              bind:value={subfolder}
              data-new-note-subfolder
            />
          {/if}
        {:else if picker === "template"}
          <div class="vault-chip-row" role="listbox" aria-label="Note kind">
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

      {#if vault.error}
        <p class="vault-interact-note text-rose-300" role="alert">{vault.error}</p>
      {/if}

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
