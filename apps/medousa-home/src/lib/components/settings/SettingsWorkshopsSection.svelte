<script lang="ts">
  import { onMount } from "svelte";
  import {
    Building2,
    FolderOpen,
    HardDrive,
    Home,
    Link2,
    Pencil,
  } from "@lucide/svelte";
  import WorkshopJoinSheet from "$lib/components/workshops/WorkshopJoinSheet.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    PERSONAL_WORKSHOP_ID,
    type WorkshopIcon,
    type WorkshopServer,
  } from "$lib/types/workshopRegistry";
  import { COLOR_THEME_OPTIONS, isColorThemeId } from "$lib/types/colorThemes";
  import { pickExternalFolder } from "$lib/utils/externalDeskApi";
  import { isTauri } from "$lib/window";

  const ICON_OPTIONS: { id: WorkshopIcon; label: string }[] = [
    { id: "home", label: "Home" },
    { id: "building", label: "Team" },
    { id: "team", label: "Group" },
  ];

  interface Props {
    onDaemonHealth?: () => void | Promise<void>;
    /** When true, this is the lead story on Connection — no top rule. */
    lead?: boolean;
  }

  let { onDaemonHealth, lead = false }: Props = $props();

  let renamingId = $state<string | null>(null);
  let renameDraft = $state("");
  let joinOpen = $state(false);
  let editingId = $state<string | null>(null);
  let brandingId = $state<string | null>(null);
  let brandColorDraft = $state("");
  let taglineDraft = $state("");
  let iconDraft = $state<WorkshopIcon>("home");
  let brandingBusy = $state(false);
  let brandingError = $state<string | null>(null);
  let addLocalOpen = $state(false);
  let localLabelDraft = $state("");
  let localDataDirDraft = $state("");
  let addLocalBusy = $state(false);
  let addLocalError = $state<string | null>(null);

  onMount(() => {
    void workshops.load();
  });

  function workshopIcon(icon: WorkshopIcon | undefined) {
    if (icon === "building" || icon === "team") return Building2;
    return Home;
  }

  function kindLabel(workshop: WorkshopServer): string {
    if (workshop.kind === "local" && workshop.id !== PERSONAL_WORKSHOP_ID) {
      return "Local engine";
    }
    return workshop.kind === "local" ? "This device" : "Paired portal";
  }

  function workshopMeta(workshop: WorkshopServer): string {
    const host = workshop.url.replace(/^https?:\/\//, "");
    const parts = [kindLabel(workshop), host];
    const theme = themeLabel(workshop.clientState?.colorThemeId);
    if (theme) parts.push(theme);
    return parts.join(" · ");
  }

  async function pickLocalDataDir() {
    const path = await pickExternalFolder();
    if (path) localDataDirDraft = path;
  }

  async function submitAddLocal() {
    const label = localLabelDraft.trim();
    const dataDir = localDataDirDraft.trim();
    if (!label || !dataDir) {
      addLocalError = "Name and engine folder are required.";
      return;
    }
    addLocalBusy = true;
    addLocalError = null;
    try {
      await workshops.addLocalEngine(label, dataDir);
      addLocalOpen = false;
      localLabelDraft = "";
      localDataDirDraft = "";
    } catch (err) {
      addLocalError = err instanceof Error ? err.message : String(err);
    } finally {
      addLocalBusy = false;
    }
  }

  function toggleEdit(workshopId: string) {
    if (editingId === workshopId) {
      editingId = null;
      brandingId = null;
      renamingId = null;
      return;
    }
    editingId = workshopId;
    brandingId = null;
    renamingId = null;
  }

  function startRename(workshop: WorkshopServer) {
    renamingId = workshop.id;
    brandingId = null;
    editingId = workshop.id;
    renameDraft = workshop.label;
  }

  async function commitRename(workshopId: string) {
    const label = renameDraft.trim();
    if (!label) {
      renamingId = null;
      return;
    }
    try {
      await workshops.renameWorkshop(workshopId, label);
    } catch {
      // Error surfaced on store.
    }
    renamingId = null;
  }

  async function switchTo(workshopId: string) {
    try {
      await workshops.selectWorkshop(workshopId, {
        onHealthChange: () => {
          void onDaemonHealth?.();
        },
      });
    } catch {
      // Error surfaced on store.
    }
  }

  function themeLabel(themeId: string | undefined): string | null {
    if (!themeId || !isColorThemeId(themeId)) return null;
    return COLOR_THEME_OPTIONS.find((option) => option.id === themeId)?.label ?? themeId;
  }

  function startBranding(workshop: WorkshopServer) {
    brandingId = workshop.id;
    renamingId = null;
    editingId = workshop.id;
    brandColorDraft = workshop.brandColor ?? "";
    taglineDraft = workshop.tagline ?? "";
    iconDraft = workshop.icon ?? (workshop.kind === "local" ? "home" : "building");
    brandingError = null;
  }

  async function saveBranding(workshopId: string) {
    brandingBusy = true;
    brandingError = null;
    try {
      await workshops.updateBranding(workshopId, {
        icon: iconDraft,
        brandColor: brandColorDraft.trim() || null,
        tagline: taglineDraft.trim() || null,
      });
      brandingId = null;
    } catch (err) {
      brandingError = err instanceof Error ? err.message : String(err);
    } finally {
      brandingBusy = false;
    }
  }

  async function removeWorkshop(workshopId: string) {
    const ok = window.confirm("Remove this workshop from the list? You can join again later.");
    if (!ok) return;
    await workshops.removeWorkshop(workshopId, { onHealthChange: onDaemonHealth });
    editingId = null;
  }
</script>

{#if isTauri()}
  <div class="ws-band" class:ws-band-lead={lead}>
    <div class="ws-band-head">
      <div class="ws-band-copy">
        <h3 class="settings-subsection-heading">{lead ? "Your workshops" : "Workshops"}</h3>
        <p class="settings-subsection-lead">
          One active connection — switch, or add another.
        </p>
      </div>
      <div class="ws-band-actions">
        <button
          type="button"
          class="ws-icon-btn"
          disabled={workshops.atWorkshopLimit}
          title="Add local engine"
          aria-label="Add local engine"
          onclick={() => {
            addLocalOpen = true;
            addLocalError = null;
          }}
        >
          <HardDrive size={15} strokeWidth={1.75} />
        </button>
        <button
          type="button"
          class="ws-icon-btn"
          disabled={workshops.atWorkshopLimit}
          title="Join paired workshop"
          aria-label="Join paired workshop"
          onclick={() => {
            joinOpen = true;
          }}
        >
          <Link2 size={15} strokeWidth={1.75} />
        </button>
      </div>
    </div>

    <div class="ws-stack">
      {#each workshops.workshops as workshop (workshop.id)}
        {@const Icon = workshopIcon(workshop.icon)}
        {@const active = workshop.id === workshops.activeWorkshopId}
        {@const editing = editingId === workshop.id}
        <div class="ws-row">
          <div class="ws-tile" class:ws-tile-active={active}>
            <span class="ws-icon" aria-hidden="true">
              <Icon size={15} strokeWidth={1.75} />
            </span>
            <span class="ws-copy">
              {#if renamingId === workshop.id}
                <input
                  class="input w-full text-sm"
                  bind:value={renameDraft}
                  aria-label="Rename workshop"
                  onkeydown={(event) => {
                    if (event.key === "Enter") void commitRename(workshop.id);
                    if (event.key === "Escape") renamingId = null;
                  }}
                />
              {:else}
                <span class="ws-title">{workshop.label}</span>
                <span class="ws-meta">{workshopMeta(workshop)}</span>
              {/if}
            </span>
            <span class="ws-tile-actions">
              {#if renamingId === workshop.id}
                <button
                  type="button"
                  class="ws-cta"
                  onclick={() => void commitRename(workshop.id)}
                >
                  Save
                </button>
                <button type="button" class="ws-cta" onclick={() => (renamingId = null)}>
                  Cancel
                </button>
              {:else}
                {#if !active}
                  <button
                    type="button"
                    class="ws-cta"
                    disabled={workshops.switching}
                    onclick={() => void switchTo(workshop.id)}
                  >
                    Switch
                  </button>
                {:else}
                  <span class="ws-pill">Active</span>
                {/if}
                <button
                  type="button"
                  class="ws-icon-btn"
                  class:ws-icon-btn-active={editing}
                  title="Edit workshop"
                  aria-label="Edit {workshop.label}"
                  aria-pressed={editing}
                  onclick={() => toggleEdit(workshop.id)}
                >
                  <Pencil size={14} strokeWidth={1.75} />
                </button>
              {/if}
            </span>
          </div>

          {#if editing && renamingId !== workshop.id}
            <div class="ws-edit">
              <div class="ws-actions">
                <button type="button" class="ws-cta" onclick={() => startRename(workshop)}>
                  Rename
                </button>
                <button type="button" class="ws-cta" onclick={() => startBranding(workshop)}>
                  Brand
                </button>
                {#if workshop.id !== PERSONAL_WORKSHOP_ID}
                  <button
                    type="button"
                    class="ws-cta ws-cta-danger"
                    disabled={workshops.switching}
                    onclick={() => void removeWorkshop(workshop.id)}
                  >
                    Remove
                  </button>
                {/if}
              </div>

              {#if brandingId === workshop.id}
                <div class="ws-brand mt-3">
                  <p class="ws-footnote">Icon</p>
                  <div class="ws-actions">
                    {#each ICON_OPTIONS as option (option.id)}
                      <button
                        type="button"
                        class="btn btn-sm {iconDraft === option.id
                          ? 'variant-filled-primary'
                          : 'variant-ghost-surface'}"
                        onclick={() => {
                          iconDraft = option.id;
                        }}
                      >
                        {option.label}
                      </button>
                    {/each}
                  </div>
                  <label class="mt-3 block">
                    <span class="workshop-label">Accent color</span>
                    <input
                      class="input mt-1 w-full font-mono text-xs"
                      placeholder="#7C3AED"
                      bind:value={brandColorDraft}
                    />
                  </label>
                  <label class="mt-3 block">
                    <span class="workshop-label">Tagline</span>
                    <input
                      class="input mt-1 w-full text-sm"
                      maxlength={80}
                      placeholder="Acme engineering brain"
                      bind:value={taglineDraft}
                    />
                  </label>
                  <p class="ws-footnote mt-2">
                    Layout theme is set in Preferences while this workshop is active.
                  </p>
                  {#if brandingError}
                    <p class="mt-2 text-xs text-warning-300">{brandingError}</p>
                  {/if}
                  <div class="ws-actions mt-3">
                    <button
                      type="button"
                      class="btn btn-sm variant-soft"
                      disabled={brandingBusy}
                      onclick={() => void saveBranding(workshop.id)}
                    >
                      Save brand
                    </button>
                    <button
                      type="button"
                      class="btn btn-sm variant-ghost-surface"
                      onclick={() => {
                        brandingId = null;
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if workshops.error}
      <p class="mt-2 text-xs text-warning-300">{workshops.error}</p>
    {/if}
  </div>
{/if}

{#if addLocalOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) addLocalOpen = false;
    }}
  >
    <div class="card w-full max-w-md space-y-4 p-5 shadow-xl" role="dialog" aria-label="Add local engine">
      <header>
        <h3 class="text-base font-semibold text-surface-50">Add local engine</h3>
        <p class="workshop-faint mt-1 text-sm">
          A second Medousa brain on this Mac with its own storage folder and port.
        </p>
      </header>
      <label class="block space-y-1 text-sm">
        <span class="text-surface-400">Name</span>
        <input class="input w-full" placeholder="Work" bind:value={localLabelDraft} />
      </label>
      <div class="space-y-1">
        <span class="text-sm text-surface-400">Engine data folder</span>
        <div class="flex gap-2">
          <input
            class="input min-w-0 flex-1 font-mono text-xs"
            placeholder="/Users/you/MedousaWork"
            bind:value={localDataDirDraft}
          />
          <button
            type="button"
            class="btn btn-sm variant-soft-surface shrink-0"
            onclick={() => void pickLocalDataDir()}
          >
            <FolderOpen size={14} strokeWidth={2} />
            Choose
          </button>
        </div>
      </div>
      {#if addLocalError}
        <p class="text-sm text-warning-300">{addLocalError}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => {
            addLocalOpen = false;
          }}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={addLocalBusy}
          onclick={() => void submitAddLocal()}
        >
          {addLocalBusy ? "Creating…" : "Create engine"}
        </button>
      </div>
    </div>
  </div>
{/if}

<WorkshopJoinSheet
  open={joinOpen}
  variant="desktop"
  onClose={() => {
    joinOpen = false;
  }}
  onHealthChange={onDaemonHealth}
/>

<style>
  .ws-band-lead,
  .ws-band:not(.ws-band-lead) {
    margin-top: 1.25rem;
  }

  .ws-band-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }

  .ws-band-copy {
    min-width: 0;
    flex: 1 1 auto;
  }

  .ws-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .ws-band-head .settings-subsection-lead {
    margin-bottom: 0;
  }

  .ws-band-actions,
  .ws-tile-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
  }

  .ws-icon-btn {
    display: inline-flex;
    height: 1.85rem;
    width: 1.85rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.28);
    color: rgb(var(--color-surface-300));
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease,
      color 120ms ease;
  }

  .ws-icon-btn:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.5);
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .ws-icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .ws-icon-btn-active {
    border-color: rgb(var(--color-primary-500) / 0.45);
    color: rgb(var(--color-primary-300));
  }

  .ws-stack {
    display: grid;
    gap: 0.5rem;
  }

  .ws-row {
    display: grid;
    gap: 0.35rem;
  }

  .ws-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .ws-tile-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
  }

  .ws-icon {
    display: flex;
    height: 1.75rem;
    width: 1.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-surface-300));
  }

  .ws-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .ws-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .ws-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .ws-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .ws-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .ws-cta-danger {
    color: rgb(var(--color-error-300) / 0.9);
  }

  .ws-pill {
    flex-shrink: 0;
    font-size: 0.65rem;
    font-weight: 600;
    color: rgb(var(--color-primary-300));
  }

  .ws-edit {
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    background: rgb(var(--color-surface-950) / 0.35);
  }

  .ws-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .ws-footnote {
    margin: 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-500));
  }
</style>
