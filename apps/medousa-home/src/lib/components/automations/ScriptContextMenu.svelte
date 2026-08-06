<script lang="ts">
  import { onMount } from "svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { scriptContextMenu } from "$lib/stores/scriptContextMenu.svelte";
  import { scriptLibrarySelection } from "$lib/stores/scriptLibrarySelection.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  let menuEl = $state<HTMLDivElement | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const target = $derived(scriptContextMenu.target);
  const deleteIds = $derived.by(() => {
    if (!target) return [] as string[];
    const multi = target.scriptIds?.filter(Boolean) ?? [];
    if (multi.length > 1) return multi;
    return target.scriptId ? [target.scriptId] : [];
  });
  const multiDelete = $derived(deleteIds.length > 1);

  function clampPosition(x: number, y: number): { x: number; y: number } {
    if (typeof window === "undefined") return { x, y };
    const width = menuEl?.offsetWidth ?? 200;
    const height = menuEl?.offsetHeight ?? (scriptContextMenu.confirmDelete ? 140 : 120);
    const margin = 8;
    return {
      x: Math.min(Math.max(margin, x), window.innerWidth - width - margin),
      y: Math.min(Math.max(margin, y), window.innerHeight - height - margin),
    };
  }

  const position = $derived(clampPosition(scriptContextMenu.x, scriptContextMenu.y));

  async function openTarget() {
    if (!target) return;
    scriptContextMenu.close();
    if (lmeWorkspace.tabs.length > 0 || lmeWorkspace.explorerMode === "scripts") {
      await lmeWorkspace.openScriptById(target.scriptId);
      return;
    }
    await graphemeScriptEditor.openScriptById(target.scriptId);
  }

  function renameTarget() {
    if (!target || multiDelete) return;
    const { scriptId } = target;
    scriptContextMenu.close();
    scriptRenameUi.startLibraryRename(scriptId);
  }

  async function confirmDelete() {
    if (!target || deleteIds.length === 0) return;
    busy = true;
    error = null;
    try {
      if (deleteIds.length === 1) {
        await workshop.deleteScript(deleteIds[0]!);
      } else {
        await workshop.deleteScripts(deleteIds);
      }
      scriptLibrarySelection.clear();
      scriptContextMenu.close();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      workshop.error = error;
    } finally {
      busy = false;
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (scriptContextMenu.open) {
        scriptContextMenu.close();
        return;
      }
      if (scriptLibrarySelection.selectedIds.size > 0) {
        scriptLibrarySelection.clear();
      }
    }
  }

  function onWindowClick(event: MouseEvent) {
    if (!scriptContextMenu.open) return;
    if (menuEl?.contains(event.target as Node)) return;
    scriptContextMenu.close();
  }

  onMount(() => {
    window.addEventListener("keydown", onWindowKeydown);
    window.addEventListener("click", onWindowClick, true);
    return () => {
      window.removeEventListener("keydown", onWindowKeydown);
      window.removeEventListener("click", onWindowClick, true);
    };
  });
</script>

{#if scriptContextMenu.open && target}
  <div
    bind:this={menuEl}
    class="vault-context-menu"
    role="menu"
    style:left="{position.x}px"
    style:top="{position.y}px"
  >
    {#if scriptContextMenu.confirmDelete}
      <div class="px-3 py-2.5">
        <p class="text-[12px] leading-snug text-surface-100">
          {#if multiDelete}
            Delete <span class="font-medium">{deleteIds.length} scripts</span>? This cannot be
            undone.
          {:else}
            Delete <span class="font-medium">{target.name}</span>? This cannot be undone.
          {/if}
        </p>
        {#if error}
          <p class="mt-1.5 text-[11px] text-content-error">{error}</p>
        {/if}
        <div class="mt-2.5 flex items-center justify-end gap-1.5">
          <button
            type="button"
            class="vault-context-menu-item !inline-flex !w-auto px-2 py-1"
            disabled={busy}
            onclick={() => {
              scriptContextMenu.confirmDelete = false;
              error = null;
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            class="vault-context-menu-item !inline-flex !w-auto px-2 py-1 text-content-error"
            disabled={busy}
            onclick={() => void confirmDelete()}
          >
            {busy ? "Deleting…" : "Delete"}
          </button>
        </div>
      </div>
    {:else if multiDelete}
      <button
        type="button"
        class="vault-context-menu-item text-content-error"
        role="menuitem"
        onclick={() => scriptContextMenu.askDelete()}
      >
        Delete {deleteIds.length} scripts…
      </button>
    {:else}
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        onclick={() => void openTarget()}
      >
        Open
      </button>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        onclick={renameTarget}
      >
        Rename…
      </button>
      <button
        type="button"
        class="vault-context-menu-item text-content-error"
        role="menuitem"
        onclick={() => scriptContextMenu.askDelete()}
      >
        Delete…
      </button>
    {/if}
  </div>
{/if}
