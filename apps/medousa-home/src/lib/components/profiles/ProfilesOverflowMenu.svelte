<script lang="ts">
  import { Ellipsis, FolderOpen, Plus, FileDown } from "@lucide/svelte";
  import { openConfigPath } from "$lib/config";
  import { exportIdentityMarkdown } from "$lib/daemon";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let open = $state(false);
  let sheetOpen = $state(false);
  let exportBusy = $state(false);
  let createSlug = $state("");
  let createName = $state("");
  let status = $state<string | null>(null);

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  async function runExport() {
    exportBusy = true;
    status = null;
    open = false;
    try {
      const result = await exportIdentityMarkdown();
      status = "Exported identity notes.";
      await openConfigPath(result.export_dir);
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
    } finally {
      exportBusy = false;
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
      class="absolute right-0 top-full z-50 mt-1 min-w-[12rem] rounded-container-token border border-surface-500/40 bg-surface-900 py-1 shadow-lg"
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
        disabled={exportBusy}
        onclick={() => void runExport()}
      >
        <FileDown size={14} aria-hidden="true" />
        {exportBusy ? "Exporting…" : "Export notes"}
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
