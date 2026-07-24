<script lang="ts">
  import { Brain, Download, Plus, UserPlus } from "@lucide/svelte";
  import { exportIdentityMarkdown } from "$lib/daemon";
  import { openConfigPath } from "$lib/config";
  import { toast } from "$lib/stores/toast.svelte";
  import {
    dispatchProfilesAddPerson,
    dispatchProfilesAddProfile,
    dispatchProfilesFocusTeach,
  } from "$lib/utils/profilesChromeEvents";

  interface Props {
    onOpenContext?: () => void;
    onAction?: () => void;
  }

  let { onOpenContext, onAction }: Props = $props();
  let exportBusy = $state(false);

  function addPerson() {
    onAction?.();
    dispatchProfilesAddPerson();
  }

  function focusTeach() {
    onAction?.();
    dispatchProfilesFocusTeach();
  }

  function addProfile() {
    onAction?.();
    dispatchProfilesAddProfile();
  }

  async function runExport() {
    exportBusy = true;
    try {
      const result = await exportIdentityMarkdown();
      await openConfigPath(result.export_dir);
      toast.show("Exported identity notes", { durationMs: 1600 });
    } catch (err) {
      toast.show(err instanceof Error ? err.message : String(err), { durationMs: 2200 });
    } finally {
      exportBusy = false;
    }
  }
</script>

<div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Add person"
  aria-label="Add person"
  onclick={addPerson}
>
  <UserPlus size={15} strokeWidth={1.75} />
</button>

<div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Teach"
    aria-label="Focus teach composer"
    onclick={focusTeach}
  >
    <span class="text-[11px] font-semibold tracking-tight">Teach</span>
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Add profile"
    aria-label="Add profile"
    onclick={addProfile}
  >
    <Plus size={15} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Export notes"
    aria-label="Export identity notes"
    disabled={exportBusy}
    onclick={() => void runExport()}
  >
    <Download size={14} strokeWidth={1.75} />
  </button>
</div>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Open Context"
  aria-label="Open Context"
  onclick={() => onOpenContext?.()}
>
  <Brain size={15} strokeWidth={1.75} />
</button>
