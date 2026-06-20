<script lang="ts">
  import { onMount } from "svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultContextMenu } from "$lib/stores/vaultContextMenu.svelte";
  import { copyTextToClipboard } from "$lib/utils/vaultClipboard";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import {
    openFileWithDefaultApp,
    openVaultNoteWithDefaultApp,
    revealFileInFinder,
    revealVaultNoteInFinder,
  } from "$lib/utils/vaultFilesystem";
  import { exportVaultNotePdf } from "$lib/utils/vaultPdfExport";
  import { isTauri } from "$lib/window";
  import { getVaultNote } from "$lib/daemon";

  interface MenuItem {
    id: string;
    label: string;
    disabled?: boolean;
    hidden?: boolean;
    onClick: () => void | Promise<void>;
  }

  let menuEl = $state<HTMLDivElement | null>(null);

  const target = $derived(vaultContextMenu.target);
  const desktopTauri = $derived(isTauri());

  function wikilinkForPath(path: string): string {
    const label =
      vault.labelByPath().get(path) ??
      vaultDisplayTitle(path.split("/").pop()?.replace(/\.md$/i, "") ?? path, path);
    const token = path.replace(/\.md$/i, "");
    return `[[${token}|${label}]]`;
  }

  function clampPosition(x: number, y: number): { x: number; y: number } {
    if (typeof window === "undefined") return { x, y };
    const width = menuEl?.offsetWidth ?? 220;
    const height = menuEl?.offsetHeight ?? 280;
    const margin = 8;
    return {
      x: Math.min(Math.max(margin, x), window.innerWidth - width - margin),
      y: Math.min(Math.max(margin, y), window.innerHeight - height - margin),
    };
  }

  const position = $derived(clampPosition(vaultContextMenu.x, vaultContextMenu.y));

  const items = $derived.by((): MenuItem[] => {
    if (!target) return [];

    if (target.kind === "note") {
      const path = target.path;
      return [
        {
          id: "copy-path",
          label: "Copy path",
          onClick: async () => {
            await copyTextToClipboard(path);
          },
        },
        {
          id: "copy-wikilink",
          label: "Copy wikilink",
          onClick: async () => {
            await copyTextToClipboard(wikilinkForPath(path));
          },
        },
        {
          id: "copy-markdown",
          label: "Copy as markdown",
          onClick: async () => {
            const content = await vault.copyNoteMarkdown(path);
            if (content) await copyTextToClipboard(content);
          },
        },
        {
          id: "duplicate",
          label: "Duplicate note",
          onClick: async () => {
            await vault.duplicateNote(path);
          },
        },
        {
          id: "rename",
          label: "Rename / move…",
          onClick: async () => {
            await vault.openNoteActionsForPath(path);
          },
        },
        {
          id: "reveal",
          label: "Reveal in Finder",
          hidden: !desktopTauri,
          onClick: async () => {
            await revealVaultNoteInFinder(path);
          },
        },
        {
          id: "open-default",
          label: "Open with default app",
          hidden: !desktopTauri,
          onClick: async () => {
            await openVaultNoteWithDefaultApp(path);
          },
        },
        {
          id: "export-pdf",
          label: "Export PDF…",
          onClick: async () => {
            const response = await getVaultNote(path);
            const title =
              vault.labelByPath().get(path) ??
              vaultDisplayTitle(response.note.title, path);
            await exportVaultNotePdf({
              title,
              content: response.content,
              labelByPath: vault.labelByPath(),
            });
          },
        },
      ];
    }

    const { path, notePath } = target;
    return [
      {
        id: "copy-path",
        label: "Copy path",
        onClick: async () => {
          await copyTextToClipboard(path);
        },
      },
      {
        id: "open",
        label: "Open file",
        onClick: async () => {
          await openFileWithDefaultApp(path);
        },
      },
      {
        id: "reveal",
        label: "Reveal in Finder",
        hidden: !desktopTauri,
        onClick: async () => {
          await revealFileInFinder(path);
        },
      },
    ];
  });

  const visibleItems = $derived(items.filter((item) => !item.hidden));

  async function runItem(item: MenuItem) {
    vaultContextMenu.close();
    await item.onClick();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") vaultContextMenu.close();
  }

  function onWindowClick(event: MouseEvent) {
    if (!vaultContextMenu.open) return;
    if (menuEl?.contains(event.target as Node)) return;
    vaultContextMenu.close();
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

{#if vaultContextMenu.open && target}
  <div
    bind:this={menuEl}
    class="vault-context-menu"
    role="menu"
    style:left="{position.x}px"
    style:top="{position.y}px"
  >
    {#each visibleItems as item (item.id)}
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={item.disabled}
        onclick={() => void runItem(item)}
      >
        {item.label}
      </button>
    {/each}
  </div>
{/if}
