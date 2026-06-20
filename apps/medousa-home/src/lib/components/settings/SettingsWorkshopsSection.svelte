<script lang="ts">
  import { onMount } from "svelte";
  import { Building2, Home, Plus, Trash2 } from "@lucide/svelte";
  import WorkshopJoinSheet from "$lib/components/workshops/WorkshopJoinSheet.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    PERSONAL_WORKSHOP_ID,
    type WorkshopIcon,
    type WorkshopServer,
  } from "$lib/types/workshopRegistry";
  import { COLOR_THEME_OPTIONS, isColorThemeId } from "$lib/types/colorThemes";
  import { isTauri } from "$lib/window";

  const ICON_OPTIONS: { id: WorkshopIcon; label: string }[] = [
    { id: "home", label: "Home" },
    { id: "building", label: "Team" },
    { id: "team", label: "Group" },
  ];

  interface Props {
    onDaemonHealth?: () => void | Promise<void>;
  }

  let { onDaemonHealth }: Props = $props();

  let renamingId = $state<string | null>(null);
  let renameDraft = $state("");
  let joinOpen = $state(false);
  let brandingId = $state<string | null>(null);
  let brandColorDraft = $state("");
  let taglineDraft = $state("");
  let iconDraft = $state<WorkshopIcon>("home");
  let brandingBusy = $state(false);
  let brandingError = $state<string | null>(null);

  onMount(() => {
    void workshops.load();
  });

  function workshopIcon(icon: WorkshopIcon | undefined) {
    if (icon === "building" || icon === "team") return Building2;
    return Home;
  }

  function kindLabel(workshop: WorkshopServer): string {
    return workshop.kind === "local" ? "This device" : "Paired phone portal";
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
  <div class="mt-8 border-t border-surface-500/35 pt-8">
    <header>
      <h3 class="text-sm font-semibold text-surface-50">Workshops</h3>
      <p class="workshop-faint mt-1 text-sm">
        Personal engine on this Mac plus paired team workshops — one active connection at a time.
      </p>
    </header>

    <button
      type="button"
      class="btn btn-sm variant-soft-primary mt-4"
      disabled={workshops.atWorkshopLimit}
      onclick={() => {
        joinOpen = true;
      }}
    >
      <Plus class="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
      Add workshop
    </button>

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

<WorkshopJoinSheet
  open={joinOpen}
  variant="desktop"
  onClose={() => {
    joinOpen = false;
  }}
  onHealthChange={onDaemonHealth}
/>
