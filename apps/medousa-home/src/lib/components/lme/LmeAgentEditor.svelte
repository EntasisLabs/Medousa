<script lang="ts">
  import SpecialistDetailEditor from "$lib/components/skills/SpecialistDetailEditor.svelte";
  import { openConfigPath } from "$lib/config";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";

  interface Props {
    onOpenChat: () => void;
  }

  let { onOpenChat }: Props = $props();

  const active = $derived(
    lmeWorkspace.activeTab?.kind === "manuscript" ? lmeWorkspace.activeTab : null,
  );

  const entry = $derived(
    active
      ? (catalog.manuscripts.find((row) => row.id === active.manuscriptId) ?? null)
      : null,
  );

  function runSkill(manuscriptId: string) {
    chat.draft = `/skill ${manuscriptId}`;
    onOpenChat();
  }

  function useInAutomation(target: ManuscriptCatalogEntry) {
    automationDraft.openCreate(
      automationDraftForSpecialist(target, catalog.manuscriptDetail),
    );
    lmeWorkspace.setExplorerMode("schedules");
    layout.navigateDesktop("library", { bump: true });
  }

  function scheduleSkill(target: ManuscriptCatalogEntry) {
    useInAutomation(target);
  }
</script>

<div
  class="lme-agent-editor flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
>
  {#if !active}
    <p class="px-5 py-5 text-sm text-surface-500 sm:px-7 sm:py-6">
      Select an agent from the side panel.
    </p>
  {:else if !entry}
    <div class="px-5 py-5 sm:px-7 sm:py-6">
      {#if catalog.manuscriptDetailLoading}
        <p class="text-sm text-surface-500">Loading agent…</p>
      {:else if catalog.manuscriptDetailError}
        <p class="text-sm text-warning-400">{catalog.manuscriptDetailError}</p>
      {:else}
        <p class="text-sm text-surface-500">
          Agent <span class="font-mono text-surface-300">{active.manuscriptId}</span> not found in
          catalog.
        </p>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface mt-3 self-start"
          onclick={() => void catalog.refresh()}
        >
          Refresh catalog
        </button>
      {/if}
    </div>
  {:else}
    <div class="flex min-h-0 min-w-0 flex-1 flex-col">
      <SpecialistDetailEditor
        {entry}
        onRunSkill={runSkill}
        onUseInAutomation={useInAutomation}
        onScheduleSkill={scheduleSkill}
        onOpenFile={(path) => void openConfigPath(path)}
      />
    </div>
  {/if}
</div>
