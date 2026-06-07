<script lang="ts">
  import { ChevronDown, ChevronUp, Search, X } from "@lucide/svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import {
    buildAskJobRequest,
    canSubmitAskJob,
    suggestedRunnableSkills,
    suggestedTools,
  } from "$lib/utils/askPrompt";
  import { filterSkills } from "$lib/utils/skillCatalog";
  import { filterTools } from "$lib/utils/toolCatalog";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  let prompt = $state("");
  let selectedSkillIds = $state<string[]>([]);
  let selectedToolIds = $state<string[]>([]);
  let pickerOpen = $state(false);
  let search = $state("");

  const skillSuggestions = $derived(
    suggestedRunnableSkills(catalog.manuscripts, 4).filter(
      (entry) => !selectedSkillIds.includes(entry.id),
    ),
  );
  const toolSuggestions = $derived(
    suggestedTools(catalog.capabilities, 4).filter(
      (entry) => !selectedToolIds.includes(entry.id),
    ),
  );

  const filteredSkills = $derived(
    filterSkills(catalog.manuscripts, search, "runnable").slice(0, 24),
  );
  const filteredTools = $derived(
    filterTools(catalog.capabilities, search, "all").slice(0, 24),
  );

  const selectedSkills = $derived(
    selectedSkillIds
      .map((id) => catalog.manuscripts.find((entry) => entry.id === id))
      .filter((entry): entry is NonNullable<typeof entry> => Boolean(entry)),
  );
  const selectedTools = $derived(
    selectedToolIds
      .map((id) => catalog.capabilities.find((entry) => entry.id === id))
      .filter((entry): entry is NonNullable<typeof entry> => Boolean(entry)),
  );

  const canSubmit = $derived(
    !workspace.askSubmitting && canSubmitAskJob(prompt, selectedSkillIds),
  );

  $effect(() => {
    if (visible && catalog.manuscripts.length === 0 && !catalog.loading) {
      void catalog.refresh();
    }
  });

  function toggleSkill(id: string) {
    if (selectedSkillIds.includes(id)) {
      selectedSkillIds = selectedSkillIds.filter((item) => item !== id);
      return;
    }
    selectedSkillIds = [...selectedSkillIds, id];
  }

  function toggleTool(id: string) {
    if (selectedToolIds.includes(id)) {
      selectedToolIds = selectedToolIds.filter((item) => item !== id);
      return;
    }
    selectedToolIds = [...selectedToolIds, id];
  }

  function removeSkill(id: string) {
    selectedSkillIds = selectedSkillIds.filter((item) => item !== id);
  }

  function removeTool(id: string) {
    selectedToolIds = selectedToolIds.filter((item) => item !== id);
  }

  function resetComposer() {
    prompt = "";
    selectedSkillIds = [];
    selectedToolIds = [];
    search = "";
    pickerOpen = false;
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (!canSubmit) return;

    const request = buildAskJobRequest(prompt, selectedSkillIds, selectedToolIds);
    try {
      await workspace.submitAsk({ ...request, modelHint: runtime.model });
      resetComposer();
    } catch {
      // workspace.askError is set in the store
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit(event);
    }
  }
</script>

{#if visible}
  <form
    class="workshop-composer shrink-0 border-t border-surface-800/80"
    onsubmit={submit}
  >
    <div class="flex flex-col gap-2 px-3 py-2">
      <div class="flex items-center justify-between gap-2">
        <button
          type="button"
          class="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-surface-400 transition hover:text-surface-200"
          onclick={() => {
            pickerOpen = !pickerOpen;
            if (!pickerOpen) search = "";
          }}
        >
          {#if pickerOpen}
            <ChevronUp size={14} />
          {:else}
            <ChevronDown size={14} />
          {/if}
          Skills & tools
          {#if selectedSkillIds.length || selectedToolIds.length}
            <span class="rounded bg-surface-800 px-1.5 py-0.5 text-[10px] normal-case text-surface-300">
              {selectedSkillIds.length + selectedToolIds.length} selected
            </span>
          {/if}
        </button>
        {#if !pickerOpen && (skillSuggestions.length > 0 || toolSuggestions.length > 0)}
          <div class="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-1">
            {#each skillSuggestions as entry (entry.id)}
              <button
                type="button"
                class="truncate rounded-md bg-surface-800/80 px-2 py-0.5 text-[10px] text-surface-300 transition hover:bg-surface-700/80"
                title={entry.description ?? entry.id}
                onclick={() => toggleSkill(entry.id)}
              >
                + {entry.name}
              </button>
            {/each}
            {#each toolSuggestions as entry (entry.id)}
              <button
                type="button"
                class="truncate rounded-md bg-surface-800/80 px-2 py-0.5 text-[10px] text-surface-300 transition hover:bg-surface-700/80"
                title={entry.description ?? entry.id}
                onclick={() => toggleTool(entry.id)}
              >
                + {entry.title}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      {#if pickerOpen}
        <label class="relative block">
          <Search
            size={14}
            class="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-surface-500"
          />
          <input
            type="search"
            class="input w-full py-1.5 pl-7 pr-2 text-xs"
            placeholder="Search skills and tools…"
            bind:value={search}
          />
        </label>

        <div class="grid max-h-36 grid-cols-2 gap-2 overflow-hidden">
          <div class="flex min-h-0 flex-col gap-1 overflow-y-auto pr-1">
            <span class="sticky top-0 bg-surface-950/95 text-[10px] font-medium uppercase tracking-wide text-surface-500">
              Skills
            </span>
            {#if catalog.loading}
              <p class="text-xs text-surface-500">Loading…</p>
            {:else if filteredSkills.length === 0}
              <p class="text-xs text-surface-500">No runnable skills match.</p>
            {:else}
              {#each filteredSkills as entry (entry.id)}
                <button
                  type="button"
                  class="rounded-md px-2 py-1 text-left text-[11px] transition {selectedSkillIds.includes(
                    entry.id,
                  )
                    ? 'bg-primary-500/20 text-primary-200 ring-1 ring-primary-500/40'
                    : 'bg-surface-800/60 text-surface-300 hover:bg-surface-700/80'}"
                  onclick={() => toggleSkill(entry.id)}
                >
                  <span class="block truncate font-medium">{entry.name}</span>
                  <span class="block truncate text-[10px] text-surface-500">{entry.id}</span>
                </button>
              {/each}
            {/if}
          </div>

          <div class="flex min-h-0 flex-col gap-1 overflow-y-auto pr-1">
            <span class="sticky top-0 bg-surface-950/95 text-[10px] font-medium uppercase tracking-wide text-surface-500">
              Tools
            </span>
            {#if catalog.loading}
              <p class="text-xs text-surface-500">Loading…</p>
            {:else if filteredTools.length === 0}
              <p class="text-xs text-surface-500">No tools match.</p>
            {:else}
              {#each filteredTools as entry (entry.id)}
                <button
                  type="button"
                  class="rounded-md px-2 py-1 text-left text-[11px] transition {selectedToolIds.includes(
                    entry.id,
                  )
                    ? 'bg-secondary-500/20 text-secondary-200 ring-1 ring-secondary-500/40'
                    : 'bg-surface-800/60 text-surface-300 hover:bg-surface-700/80'}"
                  onclick={() => toggleTool(entry.id)}
                >
                  <span class="block truncate font-medium">{entry.title}</span>
                  <span class="block truncate text-[10px] text-surface-500">{entry.id}</span>
                </button>
              {/each}
            {/if}
          </div>
        </div>
      {/if}

      {#if selectedSkills.length > 0 || selectedTools.length > 0}
        <div class="flex flex-wrap gap-1.5">
          {#each selectedSkills as entry (entry.id)}
            <span
              class="inline-flex max-w-full items-center gap-1 rounded-md bg-primary-500/15 px-2 py-0.5 text-[11px] text-primary-200 ring-1 ring-primary-500/30"
            >
              <span class="truncate">{entry.name}</span>
              <button
                type="button"
                class="shrink-0 text-primary-300/80 hover:text-primary-100"
                aria-label={`Remove skill ${entry.name}`}
                onclick={() => removeSkill(entry.id)}
              >
                <X size={12} />
              </button>
            </span>
          {/each}
          {#each selectedTools as entry (entry.id)}
            <span
              class="inline-flex max-w-full items-center gap-1 rounded-md bg-secondary-500/15 px-2 py-0.5 text-[11px] text-secondary-200 ring-1 ring-secondary-500/30"
            >
              <span class="truncate">{entry.title}</span>
              <button
                type="button"
                class="shrink-0 text-secondary-300/80 hover:text-secondary-100"
                aria-label={`Remove tool ${entry.title}`}
                onclick={() => removeTool(entry.id)}
              >
                <X size={12} />
              </button>
            </span>
          {/each}
        </div>
      {/if}

      <div class="flex items-end gap-2">
        <textarea
          class="textarea min-h-[36px] max-h-28 flex-1 resize-none py-1.5 text-sm"
          placeholder="Describe the ask — skills and tools attach as structured metadata"
          rows="1"
          bind:value={prompt}
          disabled={workspace.askSubmitting}
          onkeydown={handleKeydown}
        ></textarea>
        <button
          type="submit"
          class="btn btn-sm variant-filled-primary h-8 shrink-0 px-3"
          disabled={!canSubmit}
          aria-label="Queue ask job"
        >
          {workspace.askSubmitting ? "…" : "Run"}
        </button>
      </div>

      {#if workspace.askError}
        <p class="text-xs text-error-400">{workspace.askError}</p>
      {:else if workspace.askMessage}
        <p class="text-xs text-surface-400">{workspace.askMessage}</p>
      {/if}
    </div>
  </form>
{/if}
