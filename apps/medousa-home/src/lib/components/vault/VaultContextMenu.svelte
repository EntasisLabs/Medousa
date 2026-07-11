<script lang="ts">
  import { onMount } from "svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultContextMenu } from "$lib/stores/vaultContextMenu.svelte";
  import { vaultFolderIcons } from "$lib/stores/vaultFolderIcons.svelte";
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
  import { guessMimeFromPath, isImageAttachment } from "$lib/utils/vaultAttachments";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    ALLOWED_SURFACE_ICONS,
    SURFACE_ICON_GROUPS,
    type AllowedSurfaceIcon,
  } from "$lib/utils/environmentIconCatalog";

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
  const pickingIcon = $derived(vaultContextMenu.iconPickerKey != null);
  const activeIcon = $derived(
    vaultContextMenu.iconPickerKey
      ? vaultFolderIcons.get(vaultContextMenu.iconPickerKey)
      : null,
  );

  function wikilinkForPath(path: string): string {
    const label =
      vault.labelByPath().get(path) ??
      vaultDisplayTitle(path.split("/").pop()?.replace(/\.md$/i, "") ?? path, path);
    const token = path.replace(/\.md$/i, "");
    return `[[${token}|${label}]]`;
  }

  function clampPosition(x: number, y: number): { x: number; y: number } {
    if (typeof window === "undefined") return { x, y };
    const width = menuEl?.offsetWidth ?? (pickingIcon ? 260 : 220);
    const height = menuEl?.offsetHeight ?? (pickingIcon ? 320 : 280);
    const margin = 8;
    return {
      x: Math.min(Math.max(margin, x), window.innerWidth - width - margin),
      y: Math.min(Math.max(margin, y), window.innerHeight - height - margin),
    };
  }

  const position = $derived(clampPosition(vaultContextMenu.x, vaultContextMenu.y));

  const items = $derived.by((): MenuItem[] => {
    if (!target) return [];

    if (target.kind === "folder") {
      const { iconKey, label } = target;
      const hasCustom = Boolean(vaultFolderIcons.get(iconKey));
      return [
        {
          id: "change-icon",
          label: "Change icon…",
          onClick: () => {
            vaultContextMenu.openIconPicker(iconKey, label);
          },
        },
        {
          id: "reset-icon",
          label: "Reset icon",
          hidden: !hasCustom,
          onClick: () => {
            vaultFolderIcons.clear(iconKey);
          },
        },
      ];
    }

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
    const attachment = {
      path,
      label: path.split(/[/\\]/).pop() ?? path,
      mime: guessMimeFromPath(path),
    };
    const canEmbedImage =
      isImageAttachment(attachment) &&
      vault.selectedPath === notePath &&
      vault.editorMode === "edit" &&
      !vault.proposalActive;

    return [
      {
        id: "copy-path",
        label: "Copy path",
        onClick: async () => {
          await copyTextToClipboard(path);
        },
      },
      {
        id: "insert-embed",
        label: "Insert embed",
        hidden: !canEmbedImage,
        onClick: async () => {
          await vault.insertImageEmbed(path);
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
    if (item.id === "change-icon") {
      await item.onClick();
      return;
    }
    vaultContextMenu.close();
    await item.onClick();
  }

  function selectIcon(name: AllowedSurfaceIcon) {
    const key = vaultContextMenu.iconPickerKey;
    if (!key) return;
    vaultFolderIcons.set(key, name);
    vaultContextMenu.close();
  }

  function resetIcon() {
    const key = vaultContextMenu.iconPickerKey;
    if (!key) return;
    vaultFolderIcons.clear(key);
    vaultContextMenu.close();
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

{#if vaultContextMenu.open && (target || pickingIcon)}
  <div
    bind:this={menuEl}
    class="vault-context-menu"
    class:vault-context-menu--icons={pickingIcon}
    role="menu"
    style:left="{position.x}px"
    style:top="{position.y}px"
  >
    {#if pickingIcon}
      <div class="vault-folder-icon-picker">
        <div class="vault-folder-icon-picker-head">
          <p class="vault-folder-icon-picker-title">
            Icon for {vaultContextMenu.iconPickerLabel || "folder"}
          </p>
          {#if activeIcon}
            <button
              type="button"
              class="vault-folder-icon-picker-reset"
              onclick={resetIcon}
            >
              Reset
            </button>
          {/if}
        </div>
        {#each Object.entries(SURFACE_ICON_GROUPS) as [group, icons] (group)}
          <p class="vault-folder-icon-group">{group}</p>
          <div class="vault-folder-icon-grid">
            {#each icons as name (name)}
              {@const Icon = environmentIcon(name)}
              <button
                type="button"
                class="vault-folder-icon-option"
                class:vault-folder-icon-option--active={activeIcon === name}
                title={name}
                aria-label={name}
                onclick={() => selectIcon(name)}
              >
                <Icon size={16} strokeWidth={1.75} />
              </button>
            {/each}
          </div>
        {/each}
        <p class="vault-folder-icon-group">all</p>
        <div class="vault-folder-icon-grid">
          {#each ALLOWED_SURFACE_ICONS as name (name)}
            {@const Icon = environmentIcon(name)}
            <button
              type="button"
              class="vault-folder-icon-option"
              class:vault-folder-icon-option--active={activeIcon === name}
              title={name}
              aria-label={name}
              onclick={() => selectIcon(name)}
            >
              <Icon size={16} strokeWidth={1.75} />
            </button>
          {/each}
        </div>
      </div>
    {:else}
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
    {/if}
  </div>
{/if}
