<script lang="ts">
  import {
    Calendar,
    CalendarRange,
    FilePlus,
    FileText,
    FolderPlus,
  } from "@lucide/svelte";
  import { formatShortcut } from "$lib/platform";
  import { vault } from "$lib/stores/vault.svelte";
  import { canUseLocalVaultFilesystem } from "$lib/utils/vaultFilesystem";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();
</script>

<button
  type="button"
  role="menuitem"
  class="vault-menu-item"
  onclick={() => {
    onClose();
    vault.openNewNoteDialog();
  }}
>
  <span class="vault-create-menu__row">
    <FilePlus size={14} strokeWidth={1.6} />
    <span class="vault-create-menu__label">New note</span>
    <span class="vault-create-menu__meta">{formatShortcut("N")}</span>
  </span>
</button>

<button
  type="button"
  role="menuitem"
  class="vault-menu-item"
  disabled={vault.saving}
  onclick={() => {
    onClose();
    void vault.createDailyNote();
  }}
>
  <span class="vault-create-menu__row">
    <Calendar size={14} strokeWidth={1.6} />
    <span class="vault-create-menu__label">Daily note</span>
  </span>
</button>

<button
  type="button"
  role="menuitem"
  class="vault-menu-item"
  disabled={vault.saving}
  onclick={() => {
    onClose();
    void vault.createWeeklyReview();
  }}
>
  <span class="vault-create-menu__row">
    <CalendarRange size={14} strokeWidth={1.6} />
    <span class="vault-create-menu__label">Weekly review</span>
  </span>
</button>

<button
  type="button"
  role="menuitem"
  class="vault-menu-item"
  onclick={() => {
    onClose();
    vault.openNewGroupDialog();
  }}
>
  <span class="vault-create-menu__row">
    <FolderPlus size={14} strokeWidth={1.6} />
    <span class="vault-create-menu__label">New group</span>
  </span>
</button>

{#if canUseLocalVaultFilesystem()}
  <div class="vault-create-menu__sep" role="separator"></div>
  <button
    type="button"
    role="menuitem"
    class="vault-menu-item"
    onclick={() => {
      onClose();
      void vault.openLooseMarkdownFile();
    }}
  >
    <span class="vault-create-menu__row">
      <FileText size={14} strokeWidth={1.6} />
      <span class="vault-create-menu__label">Open markdown file…</span>
    </span>
  </button>
{/if}
