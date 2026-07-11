<script lang="ts">
  import { onMount } from "svelte";
  import { Building2, FolderOpen, Home, Plus, Trash2 } from "@lucide/svelte";
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
      return "Local engine on this Mac";
    }
    return workshop.kind === "local" ? "This device" : "Paired phone portal";
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

  function formatLastConnected(iso: string | undefined): string | null {
    if (!iso) return null;
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return null;
    return `Last connected ${date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    })}`;
  }

  function startRename(workshop: WorkshopServer) {
    renamingId = workshop.id;
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
</script>

{#if isTauri()}
  <div class={lead ? "mt-5" : "mt-8 border-t border-surface-500/35 pt-8"}>
    <header>
      <h3 class="settings-subsection-heading">{lead ? "Your workshops" : "Workshops"}</h3>
      <p class="settings-subsection-lead">
        {#if lead}
          One active connection at a time — switch here, or add another engine / paired workshop.
        {:else}
          Personal engine on this Mac plus paired team workshops — one active connection at a time.
        {/if}
      </p>
    </header>

    <div class="mt-4 flex flex-wrap gap-2">
      <button
        type="button"
        class="btn btn-sm variant-soft-primary"
        disabled={workshops.atWorkshopLimit}
        onclick={() => {
          addLocalOpen = true;
          addLocalError = null;
        }}
      >
        <Plus class="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
        Add local engine
      </button>
      <button
        type="button"
        class="btn btn-sm variant-soft-surface"
        disabled={workshops.atWorkshopLimit}
        onclick={() => {
          joinOpen = true;
        }}
      >
        Join paired workshop
      </button>
    </div>

    {#if workshops.error}
      <p class="mt-4 text-sm text-error-400">{workshops.error}</p>
    {/if}

    <ul class="mt-4 space-y-2">
      {#each workshops.workshops as workshop (workshop.id)}
        {@const Icon = workshopIcon(workshop.icon)}
        <li
          class="rounded-xl border border-surface-500/30 bg-surface-950/40 px-3 py-3 {workshop.id ===
          workshops.activeWorkshopId
            ? 'border-primary-500/35'
            : ''}"
        >
          <div class="flex items-start gap-3">
            <span
              class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-800/80 text-surface-200"
              aria-hidden="true"
            >
              <Icon size={16} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              {#if renamingId === workshop.id}
                <input
                  class="input w-full text-sm"
                  bind:value={renameDraft}
                  onkeydown={(event) => {
                    if (event.key === "Enter") void commitRename(workshop.id);
                    if (event.key === "Escape") renamingId = null;
                  }}
                />
                <div class="mt-2 flex gap-2">
                  <button
                    type="button"
                    class="btn btn-sm variant-filled-primary"
                    onclick={() => commitRename(workshop.id)}
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    class="btn btn-sm variant-ghost-surface"
                    onclick={() => {
                      renamingId = null;
                    }}
                  >
                    Cancel
                  </button>
                </div>
              {:else}
                <p class="text-sm font-medium text-surface-50">{workshop.label}</p>
                <p class="workshop-faint mt-0.5 text-xs">
                  {kindLabel(workshop)} · {workshop.url.replace(/^https?:\/\//, "")}
                </p>
                {#if workshop.dataDir}
                  <p class="workshop-faint mt-1 break-all font-mono text-[10px]">
                    {workshop.dataDir}
                  </p>
                {/if}
                {#if formatLastConnected(workshop.lastConnectedAt)}
                  <p class="workshop-faint mt-1 text-[11px]">
                    {formatLastConnected(workshop.lastConnectedAt)}
                  </p>
                {/if}
                {#if workshop.id === workshops.activeWorkshopId}
                  <span class="badge variant-soft-primary mt-2 text-[10px]">Active</span>
                {/if}
                {#if themeLabel(workshop.clientState?.colorThemeId)}
                  <p class="workshop-faint mt-1 text-[11px]">
                    Room theme · {themeLabel(workshop.clientState?.colorThemeId)}
                  </p>
                {/if}
                {#if workshop.tagline}
                  <p class="mt-1 text-xs text-surface-300">{workshop.tagline}</p>
                {/if}
              {/if}
            </div>
            {#if renamingId !== workshop.id}
              <div class="flex shrink-0 flex-col items-end gap-1">
                {#if workshop.id !== workshops.activeWorkshopId}
                  <button
                    type="button"
                    class="btn btn-sm variant-soft-surface"
                    disabled={workshops.switching}
                    onclick={() => switchTo(workshop.id)}
                  >
                    Switch
                  </button>
                {/if}
                <button
                  type="button"
                  class="workshop-text-action text-xs"
                  onclick={() => startRename(workshop)}
                >
                  Rename
                </button>
                <button
                  type="button"
                  class="workshop-text-action text-xs"
                  onclick={() => startBranding(workshop)}
                >
                  Brand
                </button>
                {#if workshop.id !== PERSONAL_WORKSHOP_ID}
                  <button
                    type="button"
                    class="inline-flex items-center gap-1 text-xs text-error-400/90 hover:text-error-300"
                    disabled={workshops.switching}
                    onclick={() => workshops.removeWorkshop(workshop.id, { onHealthChange: onDaemonHealth })}
                  >
                    <Trash2 size={12} strokeWidth={2} />
                    Remove
                  </button>
                {/if}
              </div>
            {/if}
          </div>
          {#if brandingId === workshop.id}
            <div class="mt-3 space-y-3 border-t border-surface-500/30 pt-3">
              <div>
                <span class="workshop-label">Icon</span>
                <div class="mt-1 flex flex-wrap gap-1">
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
              </div>
              <label class="block">
                <span class="workshop-label">Accent color</span>
                <input
                  class="input mt-1 w-full font-mono text-xs"
                  placeholder="#7C3AED"
                  bind:value={brandColorDraft}
                />
              </label>
              <label class="block">
                <span class="workshop-label">Tagline</span>
                <input
                  class="input mt-1 w-full text-sm"
                  maxlength={80}
                  placeholder="Acme engineering brain"
                  bind:value={taglineDraft}
                />
              </label>
              <p class="workshop-faint text-xs">
                Room theme is set in Settings → Room while this workshop is active.
              </p>
              {#if brandingError}
                <p class="text-xs text-error-400">{brandingError}</p>
              {/if}
              <div class="flex gap-2">
                <button
                  type="button"
                  class="btn btn-sm variant-filled-primary"
                  disabled={brandingBusy}
                  onclick={() => saveBranding(workshop.id)}
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
        </li>
      {/each}
    </ul>
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
        <p class="text-sm text-error-400">{addLocalError}</p>
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
