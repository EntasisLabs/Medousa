<script lang="ts">
  import { Check, ChevronDown, FolderOpen, Plus } from "@lucide/svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { pickExternalFolder, rootLabelFromPath } from "$lib/utils/externalDeskApi";
  import {
    isCoLocatedWorkshop,
    vaultAddRootRemoteHint,
  } from "$lib/utils/workshopLocality";
  import { isTauri } from "$lib/window";

  interface Props {
    compact?: boolean;
    /** Text-style trigger — no filled control. */
    quiet?: boolean;
    /** Open the menu above the trigger (e.g. bottom dock). */
    dropUp?: boolean;
  }

  let { compact = false, quiet = false, dropUp = false }: Props = $props();

  let menuOpen = $state(false);
  let addBusy = $state(false);
  let addError = $state<string | null>(null);
  let labelDraft = $state("");
  let pathDraft = $state("");

  const activeRoot = $derived(vault.activeVaultRootView);
  const coLocated = $derived(isCoLocatedWorkshop());
  const canAddRoot = $derived(
    isTauri() && coLocated && !vault.vaultRootsUnavailable,
  );
  const showPicker = $derived(!vault.vaultRootsUnavailable && vault.vaultRoots.length > 0);

  async function pickFolder() {
    const path = await pickExternalFolder();
    if (!path) return;
    pathDraft = path;
    if (!labelDraft.trim()) {
      labelDraft = rootLabelFromPath(path);
    }
  }

  async function submitAddRoot() {
    const label = labelDraft.trim();
    const path = pathDraft.trim();
    if (!label || !path) {
      addError = "Name and folder are required.";
      return;
    }
    addBusy = true;
    addError = null;
    try {
      await vault.registerVaultRoot(label, path);
      labelDraft = "";
      pathDraft = "";
      vault.closeAddVaultRootDialog();
    } catch (err) {
      addError = err instanceof Error ? err.message : String(err);
    } finally {
      addBusy = false;
    }
  }

  async function selectRoot(rootId: string) {
    menuOpen = false;
    if (rootId === vault.activeVaultRootId) return;
    try {
      await vault.switchVaultRoot(rootId);
    } catch {
      // vault.error is set in store
    }
  }

  function truncatePath(path: string): string {
    if (path.length <= 42) return path;
    const parts = path.split("/");
    if (parts.length <= 3) return path;
    return `…/${parts.slice(-2).join("/")}`;
  }
</script>

{#if vault.vaultRootsUnavailable}
  <span class="workshop-faint text-xs" title="Restart or update the engine for multi-vault support">
    Personal vault
  </span>
{:else if compact}
  <div class="relative min-w-0 {quiet ? '' : 'w-full'}">
    <button
      type="button"
      class={quiet
        ? "workshop-text-action inline-flex max-w-full items-center gap-1 text-xs text-surface-400"
        : "vault-root-trigger vault-root-trigger-fill"}
      aria-haspopup="listbox"
      aria-expanded={menuOpen}
      disabled={vault.vaultRootsLoading}
      onclick={() => {
        menuOpen = !menuOpen;
      }}
    >
      <FolderOpen size={12} strokeWidth={2} class="shrink-0 opacity-70" />
      <span class="truncate">
          {#if vault.vaultRootsLoading}
            …
          {:else}
            {activeRoot?.label ?? "Personal"}{#if activeRoot?.isObsidian}
              <span class="workshop-faint"> · Obsidian</span>
            {/if}
          {/if}
      </span>
      <ChevronDown size={12} strokeWidth={2} class="shrink-0 opacity-60" />
    </button>

    {#if menuOpen}
      <div
        class="absolute left-0 z-30 w-full min-w-[12rem] rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl {dropUp
          ? 'bottom-full mb-1'
          : 'top-full mt-1'}"
        role="listbox"
        aria-label="Vault folders"
      >
        {#each vault.vaultRoots as root (root.id)}
          <button
            type="button"
            role="option"
            aria-selected={root.active}
            class="flex w-full items-start gap-2 px-3 py-2 text-left text-sm hover:bg-surface-800/80 {root.active
              ? 'bg-primary-500/10'
              : ''}"
            onclick={() => void selectRoot(root.id)}
          >
            <Check
              size={14}
              strokeWidth={2}
              class="mt-0.5 shrink-0 {root.active ? 'text-primary-300' : 'opacity-0'}"
            />
            <span class="min-w-0 flex-1">
              <span class="flex items-center gap-1.5">
                <span class="block font-medium text-surface-100">{root.label}</span>
                {#if root.isObsidian}
                  <span class="badge variant-soft-surface px-1.5 py-0 text-[10px] font-medium">
                    Obsidian
                  </span>
                {/if}
              </span>
              <span class="workshop-faint mt-0.5 block truncate font-mono text-[10px]" title={root.path}>
                {truncatePath(root.path)}
              </span>
            </span>
          </button>
        {/each}
        {#if canAddRoot}
          <button
            type="button"
            class="flex w-full items-center gap-2 border-t border-surface-500/40 px-3 py-2 text-left text-sm text-surface-200 hover:bg-surface-800/80"
            onclick={() => {
              menuOpen = false;
              addError = null;
              vault.openAddVaultRootDialog();
            }}
          >
            <Plus size={14} strokeWidth={2} />
            Add vault folder…
          </button>
        {:else if isTauri() && !coLocated && !vault.vaultRootsUnavailable}
          <p class="workshop-faint border-t border-surface-500/40 px-3 py-2 text-[11px] leading-snug">
            {vaultAddRootRemoteHint()}
          </p>
        {/if}
      </div>
    {/if}
  </div>
{:else if showPicker}
  <div class="px-3 pb-2 pt-2">
    <div class="relative min-w-0">
      <button
        type="button"
        class="input flex w-full items-center justify-between gap-2 py-1.5 text-left text-sm"
        aria-haspopup="listbox"
        aria-expanded={menuOpen}
        disabled={vault.vaultRootsLoading}
        onclick={() => {
          menuOpen = !menuOpen;
        }}
      >
        <span class="truncate">
          {#if vault.vaultRootsLoading}
            Loading vaults…
          {:else}
            {activeRoot?.label ?? "Personal"}{#if activeRoot?.isObsidian}
              <span class="workshop-faint"> · Obsidian</span>
            {/if}
          {/if}
        </span>
        <ChevronDown size={14} strokeWidth={2} class="shrink-0 opacity-70" />
      </button>

      {#if menuOpen}
        <div
          class="absolute left-0 right-0 z-20 mt-1 rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
          role="listbox"
        >
          {#each vault.vaultRoots as root (root.id)}
            <button
              type="button"
              role="option"
              aria-selected={root.active}
              class="flex w-full items-start gap-2 px-3 py-2 text-left text-sm hover:bg-surface-800/80"
              onclick={() => void selectRoot(root.id)}
            >
              <Check
                size={14}
                strokeWidth={2}
                class="mt-0.5 shrink-0 {root.active ? 'text-primary-300' : 'opacity-0'}"
              />
              <span class="min-w-0">
                <span class="flex items-center gap-1.5">
                  <span class="font-medium">{root.label}</span>
                  {#if root.isObsidian}
                    <span class="badge variant-soft-surface px-1.5 py-0 text-[10px] font-medium">
                      Obsidian
                    </span>
                  {/if}
                </span>
                <span class="workshop-faint mt-0.5 block truncate font-mono text-[10px]" title={root.path}>
                  {truncatePath(root.path)}
                </span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    {#if vault.vaultRootsError}
      <p class="mt-1 text-xs text-error-400">{vault.vaultRootsError}</p>
    {/if}
  </div>
{/if}

{#if vault.addVaultRootOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) vault.closeAddVaultRootDialog();
    }}
  >
    <div class="card w-full max-w-md space-y-4 p-5 shadow-xl" role="dialog" aria-label="Add vault folder">
      <header>
        <h3 class="text-base font-semibold text-surface-50">Add vault folder</h3>
        <p class="workshop-faint mt-1 text-sm">
          Point this engine at another markdown folder on this Mac — including an Obsidian vault
          (folder with <code class="font-mono text-[11px]">.obsidian</code>). Switch vaults anytime from the library sidebar.
        </p>
      </header>
      <label class="block space-y-1 text-sm">
        <span class="text-surface-400">Name</span>
        <input class="input w-full" placeholder="Work notes" bind:value={labelDraft} />
      </label>
      <div class="space-y-1">
        <span class="text-sm text-surface-400">Folder</span>
        <div class="flex gap-2">
          <input
            class="input min-w-0 flex-1 font-mono text-xs"
            placeholder="/Users/you/WorkVault"
            bind:value={pathDraft}
          />
          <button
            type="button"
            class="btn btn-sm variant-soft-surface shrink-0"
            onclick={() => void pickFolder()}
          >
            <FolderOpen size={14} strokeWidth={2} />
            Choose
          </button>
        </div>
      </div>
      {#if addError}
        <p class="text-sm text-error-400">{addError}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => vault.closeAddVaultRootDialog()}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={addBusy}
          onclick={() => void submitAddRoot()}
        >
          {addBusy ? "Adding…" : "Add vault"}
        </button>
      </div>
    </div>
  </div>
{/if}
