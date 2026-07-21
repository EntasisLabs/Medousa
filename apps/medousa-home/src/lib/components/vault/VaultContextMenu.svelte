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
  import { isTauri } from "$lib/window";
  import { getVaultNote } from "$lib/daemon";
  import { guessMimeFromPath, isImageAttachment } from "$lib/utils/vaultAttachments";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    ALLOWED_SURFACE_ICONS,
    SURFACE_ICON_GROUPS,
    type AllowedSurfaceIcon,
  } from "$lib/utils/environmentIconCatalog";
  import { addVaultSelectionToChat } from "$lib/utils/vaultNoteWorkshop";
  import { canUseLocalVaultFilesystem } from "$lib/utils/vaultFilesystem";
  import { vaultHostSideHint } from "$lib/utils/workshopLocality";
  import { revealInFileManagerLabel } from "$lib/platformCopy";
  import type { VaultExportFormat } from "$lib/utils/vaultExportOptions";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import VaultExportPreviewModal from "./VaultExportPreviewModal.svelte";

  const MENU_ACTION_TIMEOUT_MS = 8000;

  interface MenuItem {
    id: string;
    label: string;
    disabled?: boolean;
    hidden?: boolean;
    separatorBefore?: boolean;
    onClick: () => void | Promise<void>;
  }

  let menuEl = $state<HTMLDivElement | null>(null);
  let exportPreviewOpen = $state(false);
  let exportPreviewFormat = $state<VaultExportFormat>("pdf");
  let exportPreviewTitle = $state("");
  let exportPreviewContent = $state("");
  let exportPreviewLabels = $state<Map<string, string>>(new Map());
  let exportPreviewPath = $state<string | null>(null);

  const target = $derived(vaultContextMenu.target);
  const desktopTauri = $derived(isTauri());
  const localFs = $derived(canUseLocalVaultFilesystem());
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

    if (target.kind === "editor") {
      const path = target.path;
      const selection = target.selection;
      const { actions } = target;
      const hasSel = actions.hasSelection;
      return [
        {
          id: "cut",
          label: "Cut",
          disabled: !hasSel,
          onClick: () => actions.cut(),
        },
        {
          id: "copy",
          label: "Copy",
          disabled: !hasSel,
          onClick: () => actions.copy(),
        },
        {
          id: "paste",
          label: "Paste",
          onClick: () => actions.paste(),
        },
        {
          id: "select-all",
          label: "Select all",
          onClick: () => actions.selectAll(),
        },
        {
          id: "bold",
          label: "Bold",
          separatorBefore: true,
          hidden: !actions.canFormat,
          disabled: !hasSel,
          onClick: () => actions.format("bold"),
        },
        {
          id: "italic",
          label: "Italic",
          hidden: !actions.canFormat,
          disabled: !hasSel,
          onClick: () => actions.format("italic"),
        },
        {
          id: "code",
          label: "Inline code",
          hidden: !actions.canFormat,
          disabled: !hasSel,
          onClick: () => actions.format("code"),
        },
        {
          id: "insert-wikilink",
          label: "Insert wikilink…",
          separatorBefore: true,
          onClick: () => actions.insertWikilink(),
        },
        {
          id: "add-to-chat",
          label: "Add to chat",
          hidden: !selection?.text.trim(),
          onClick: async () => {
            if (!selection?.text.trim()) return;
            await addVaultSelectionToChat({
              path,
              title:
                vault.selectedPath === path
                  ? vault.title
                  : (vault.labelByPath().get(path) ?? path),
              content: vault.selectedPath === path ? vault.content : undefined,
              wikilinksOut: vault.selectedPath === path ? vault.wikilinksOut : [],
              backlinks: vault.selectedPath === path ? vault.backlinks : [],
              selection,
              flushSave:
                vault.selectedPath === path && vault.dirty
                  ? async () => {
                      await vault.flushSave();
                    }
                  : undefined,
            });
          },
        },
        {
          id: "copy-path",
          label: "Copy path",
          separatorBefore: true,
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
          id: "rename",
          label: "Rename / move…",
          onClick: async () => {
            await vault.openNoteActionsForPath(path);
          },
        },
      ];
    }

    if (target.kind === "note") {
      const path = target.path;
      const selection = target.selection;
      return [
        {
          id: "add-to-chat",
          label: "Add to chat",
          hidden: !selection?.text.trim(),
          onClick: async () => {
            if (!selection?.text.trim()) return;
            await addVaultSelectionToChat({
              path,
              title:
                vault.selectedPath === path
                  ? vault.title
                  : (vault.labelByPath().get(path) ?? path),
              content: vault.selectedPath === path ? vault.content : undefined,
              wikilinksOut: vault.selectedPath === path ? vault.wikilinksOut : [],
              backlinks: vault.selectedPath === path ? vault.backlinks : [],
              selection,
              flushSave:
                vault.selectedPath === path && vault.dirty
                  ? async () => {
                      await vault.flushSave();
                    }
                  : undefined,
            });
          },
        },
        {
          id: "copy-selection",
          label: "Copy selection",
          hidden: !selection?.text.trim(),
          onClick: async () => {
            if (selection?.text) await copyTextToClipboard(selection.text);
          },
        },
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
          label: revealInFileManagerLabel(),
          hidden: !localFs,
          onClick: async () => {
            await revealVaultNoteInFinder(path);
          },
        },
        {
          id: "open-default",
          label: "Open with default app",
          hidden: !localFs,
          onClick: async () => {
            await openVaultNoteWithDefaultApp(path);
          },
        },
        {
          id: "host-hint",
          label: vaultHostSideHint(),
          hidden: localFs || !desktopTauri,
          disabled: true,
          onClick: () => {},
        },
        {
          id: "export-pdf",
          label: "Export PDF…",
          onClick: async () => {
            try {
              const response = await getVaultNote(path);
              const title =
                vault.labelByPath().get(path) ??
                vaultDisplayTitle(response.note.title, path);
              exportPreviewTitle = title;
              exportPreviewContent = response.content;
              exportPreviewLabels = vault.labelByPath();
              exportPreviewPath = path;
              exportPreviewFormat = "pdf";
              exportPreviewOpen = true;
            } catch (err) {
              vault.error = err instanceof Error ? err.message : String(err);
            }
          },
        },
        {
          id: "export-word",
          label: "Export Word…",
          onClick: async () => {
            try {
              const response = await getVaultNote(path);
              const title =
                vault.labelByPath().get(path) ??
                vaultDisplayTitle(response.note.title, path);
              exportPreviewTitle = title;
              exportPreviewContent = response.content;
              exportPreviewLabels = vault.labelByPath();
              exportPreviewPath = path;
              exportPreviewFormat = "docx";
              exportPreviewOpen = true;
            } catch (err) {
              vault.error = err instanceof Error ? err.message : String(err);
            }
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
        hidden: !canEmbedImage || !localFs,
        onClick: async () => {
          await vault.insertImageEmbed(path);
        },
      },
      {
        id: "open",
        label: "Open file",
        hidden: !localFs && !path.startsWith("http://") && !path.startsWith("https://"),
        onClick: async () => {
          await openFileWithDefaultApp(path);
        },
      },
      {
        id: "reveal",
        label: revealInFileManagerLabel(),
        hidden: !localFs,
        onClick: async () => {
          await revealFileInFinder(path);
        },
      },
      {
        id: "host-hint",
        label: vaultHostSideHint(),
        hidden: localFs || !desktopTauri,
        disabled: true,
        onClick: () => {},
      },
    ];
  });

  const visibleItems = $derived(items.filter((item) => !item.hidden));

  async function runItem(item: MenuItem) {
    if (item.disabled) return;
    if (item.id === "change-icon") {
      await item.onClick();
      return;
    }
    // Run the action before closing when possible so clipboard paste keeps a
    // user-activation gesture on WebView2; still hard-timeout so Windows
    // clipboard permission dialogs cannot freeze the shell forever.
    try {
      await Promise.race([
        Promise.resolve(item.onClick()),
        new Promise<never>((_, reject) => {
          setTimeout(() => reject(new Error("menu-action-timeout")), MENU_ACTION_TIMEOUT_MS);
        }),
      ]);
    } catch {
      toast.show("That action timed out — try the keyboard shortcut", {
        durationMs: 3200,
      });
    } finally {
      vaultContextMenu.close();
    }
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

  function onWindowPointerDown(event: PointerEvent) {
    if (!vaultContextMenu.open) return;
    // Ignore the opening right-click / aux-button sequence on Windows WebView2.
    if (event.button === 2) return;
    if (menuEl?.contains(event.target as Node)) return;
    vaultContextMenu.close();
  }

  onMount(() => {
    window.addEventListener("keydown", onWindowKeydown);
    window.addEventListener("pointerdown", onWindowPointerDown, true);
    return () => {
      window.removeEventListener("keydown", onWindowKeydown);
      window.removeEventListener("pointerdown", onWindowPointerDown, true);
    };
  });
</script>

{#if vaultContextMenu.open && (target || pickingIcon)}
  <BodyPortal>
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
          {#if item.separatorBefore}
            <div class="vault-context-menu-sep" role="separator"></div>
          {/if}
          <button
            type="button"
            class="vault-context-menu-item"
            class:vault-context-menu-item--hint={item.disabled && item.id === "host-hint"}
            role="menuitem"
            disabled={item.disabled}
            onclick={() => void runItem(item)}
          >
            {item.label}
          </button>
        {/each}
      {/if}
    </div>
  </BodyPortal>
{/if}

<VaultExportPreviewModal
  open={exportPreviewOpen}
  title={exportPreviewTitle}
  content={exportPreviewContent}
  labelByPath={exportPreviewLabels}
  notePath={exportPreviewPath}
  initialFormat={exportPreviewFormat}
  onClose={() => {
    exportPreviewOpen = false;
  }}
/>
