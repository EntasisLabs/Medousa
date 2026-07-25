<script lang="ts">
  import { Ellipsis, FileDown, FileUp, Plus } from "@lucide/svelte";
  import { openConfigPath } from "$lib/config";
  import {
    exportIdentityMarkdown,
    exportUserProfileBundle,
    importUserProfileBundle,
  } from "$lib/daemon";
  import { toast } from "$lib/stores/toast.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { downloadTextFile } from "$lib/utils/sessionTranscript";
  import { PROFILES_ADD_PROFILE_EVENT } from "$lib/utils/profilesChromeEvents";
  import { onMount } from "svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let open = $state(false);
  let sheetOpen = $state(false);
  let importInputEl = $state<HTMLInputElement | null>(null);

  onMount(() => {
    const onAddProfile = () => {
      open = false;
      sheetOpen = true;
    };
    window.addEventListener(PROFILES_ADD_PROFILE_EVENT, onAddProfile);
    return () => window.removeEventListener(PROFILES_ADD_PROFILE_EVENT, onAddProfile);
  });
  let exportBusy = $state(false);
  let backupBusy = $state(false);
  let createSlug = $state("");
  let createName = $state("");
  let status = $state<string | null>(null);

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  async function runExportNotes() {
    exportBusy = true;
    status = null;
    open = false;
    try {
      const result = await exportIdentityMarkdown();
      status = "Exported identity notes.";
      await openConfigPath(result.export_dir);
      toast.show("Exported identity notes", { durationMs: 1800 });
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
      toast.show(status, { durationMs: 2400 });
    } finally {
      exportBusy = false;
    }
  }

  async function runExportBackup() {
    const profileId = userProfiles.activeProfileId?.trim();
    if (!profileId) {
      toast.show("No active profile to export", { durationMs: 1800 });
      return;
    }
    backupBusy = true;
    status = null;
    open = false;
    try {
      const result = await exportUserProfileBundle({ profileId });
      const label =
        userProfiles.profiles
          .find((p) => p.profile_id === profileId)
          ?.display_name?.trim()
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "") || profileId.slice(0, 8);
      downloadTextFile(
        `medousa-profile-${label}.json`,
        JSON.stringify(result.bundle, null, 2),
        "application/json",
      );
      status = "Exported profile backup.";
      toast.show("Profile backup downloaded", { durationMs: 1800 });
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
      toast.show(status, { durationMs: 2400 });
    } finally {
      backupBusy = false;
    }
  }

  function pickImportBackup() {
    open = false;
    importInputEl?.click();
  }

  async function onImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    backupBusy = true;
    status = null;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as unknown;
      const bundle =
        parsed &&
        typeof parsed === "object" &&
        "bundle" in parsed &&
        (parsed as { bundle: unknown }).bundle
          ? (parsed as { bundle: unknown }).bundle
          : parsed;
      const result = await importUserProfileBundle({ bundle, dryRun: false });
      await userProfiles.load({ suppressRemoteNotice: true });
      status = result.message || "Imported profile backup.";
      toast.show(status, { durationMs: 2200 });
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
      toast.show(status, { durationMs: 2600 });
    } finally {
      backupBusy = false;
    }
  }

  async function submitCreate(event: SubmitEvent) {
    event.preventDefault();
    const ok = await userProfiles.create(createSlug, createName);
    if (ok) {
      createSlug = "";
      createName = "";
      sheetOpen = false;
    }
  }
</script>

<input
  bind:this={importInputEl}
  type="file"
  accept="application/json,.json"
  class="hidden"
  onchange={(event) => void onImportFile(event)}
/>

<div class="relative">
  <button
    type="button"
    class="btn btn-sm variant-ghost-surface shrink-0"
    aria-label="More profile actions"
    aria-expanded={open}
    disabled={readOnly}
    onclick={() => {
      open = !open;
      sheetOpen = false;
    }}
  >
    <Ellipsis size={16} strokeWidth={1.75} />
  </button>

  {#if open}
    <button
      type="button"
      class="fixed inset-0 z-40 cursor-default"
      aria-label="Close menu"
      onclick={() => {
        open = false;
      }}
    ></button>
    <div
      class="absolute right-0 top-full z-50 mt-1 min-w-[14rem] rounded-container-token border border-surface-500/40 bg-surface-900 py-1 shadow-lg"
      role="menu"
    >
      <button
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-surface-200 hover:bg-surface-800/80"
        role="menuitem"
        onclick={() => {
          open = false;
          sheetOpen = true;
        }}
      >
        <Plus size={14} aria-hidden="true" />
        Add profile
      </button>
      <button
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-surface-200 hover:bg-surface-800/80"
        role="menuitem"
        disabled={backupBusy}
        onclick={() => void runExportBackup()}
      >
        <FileDown size={14} aria-hidden="true" />
        {backupBusy ? "Exporting…" : "Export profile backup…"}
      </button>
      <button
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-surface-200 hover:bg-surface-800/80"
        role="menuitem"
        disabled={backupBusy}
        onclick={() => pickImportBackup()}
      >
        <FileUp size={14} aria-hidden="true" />
        Import profile backup…
      </button>
      <button
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-surface-200 hover:bg-surface-800/80"
        role="menuitem"
        disabled={exportBusy}
        onclick={() => void runExportNotes()}
      >
        <FileDown size={14} aria-hidden="true" />
        {exportBusy ? "Exporting…" : "Export identity notes"}
      </button>
    </div>
  {/if}
</div>

{#if sheetOpen}
  <div
    class="mobile-sheet-backdrop z-50"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) sheetOpen = false;
    }}
  >
    <div class="mobile-sheet" role="dialog" aria-label="Add profile">
      <header class="mobile-sheet-header">
        <h2 class="text-sm font-semibold text-surface-50">Add profile</h2>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => {
            sheetOpen = false;
          }}
        >
          Cancel
        </button>
      </header>
      <form class="space-y-3 px-4 pb-6 pt-2" onsubmit={submitCreate}>
        <label class="block">
          <span class="workshop-label">Short id</span>
          <input class="input mt-1 w-full text-sm" placeholder="work" bind:value={createSlug} />
        </label>
        <label class="block">
          <span class="workshop-label">Name</span>
          <input class="input mt-1 w-full text-sm" placeholder="Work" bind:value={createName} />
        </label>
        <button
          type="submit"
          class="btn btn-sm variant-filled-primary"
          disabled={userProfiles.saving}
        >
          Create
        </button>
      </form>
    </div>
  </div>
{/if}

{#if status}
  <p class="sr-only" role="status">{status}</p>
{/if}
