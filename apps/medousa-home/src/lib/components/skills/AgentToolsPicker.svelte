<script lang="ts">
  import { groupAgentTools } from "$lib/utils/agentToolCategories";

  interface Props {
    palette: string[];
    selected: string[];
    onToggle: (toolId: string, enabled: boolean) => void;
    /** Stretch the module/tool split to fill a side column. */
    fill?: boolean;
  }

  let { palette, selected, onToggle, fill = false }: Props = $props();

  let search = $state("");
  let selectedModuleId = $state<string | null>(null);

  const categories = $derived(groupAgentTools(palette, selected, search));

  $effect(() => {
    if (categories.length === 0) {
      selectedModuleId = null;
      return;
    }
    if (!selectedModuleId || !categories.some((c) => c.moduleId === selectedModuleId)) {
      selectedModuleId = categories[0]!.moduleId;
    }
  });

  const activeCategory = $derived(
    categories.find((c) => c.moduleId === selectedModuleId) ?? null,
  );

  const selectedCount = $derived(selected.length);
</script>

<div class="agent-tools-picker flex min-h-0 flex-1 flex-col {fill ? 'h-full' : ''}">
  <div class="flex shrink-0 items-center justify-between gap-2 px-0.5">
    <p class="text-xs text-surface-400">Modules → tools</p>
    <span class="text-[11px] tabular-nums text-surface-500">{selectedCount} selected</span>
  </div>

  <input
    class="agent-liquid-input mt-2 w-full shrink-0 text-sm"
    type="search"
    placeholder="Search modules or tools…"
    bind:value={search}
    aria-label="Filter tools"
  />

  {#if categories.length === 0}
    <p class="mt-4 text-xs text-surface-500">No tools match.</p>
  {:else}
    <div
      class="agent-tools-split mt-3 flex min-h-[14rem] flex-1 overflow-hidden rounded-xl border border-surface-500/30 bg-surface-950/40 {fill
        ? 'min-h-[18rem]'
        : ''}"
    >
      <ul
        class="agent-tools-modules w-[38%] shrink-0 overflow-y-auto border-r border-surface-500/30"
        role="listbox"
        aria-label="Tool modules"
      >
        {#each categories as category (category.moduleId)}
          <li role="presentation">
            <button
              type="button"
              role="option"
              aria-selected={selectedModuleId === category.moduleId}
              class="flex w-full items-center gap-2 px-3 py-2 text-left transition {selectedModuleId ===
              category.moduleId
                ? 'bg-surface-800/90 text-surface-50'
                : 'text-surface-400 hover:bg-surface-800/50 hover:text-surface-200'}"
              onclick={() => (selectedModuleId = category.moduleId)}
            >
              <span class="min-w-0 flex-1 truncate text-[12px] font-medium">{category.label}</span>
              <span class="shrink-0 text-[10px] tabular-nums text-surface-500">
                {#if category.selectedCount > 0}
                  <span class="text-primary-300">{category.selectedCount}</span
                  >/{category.tools.length}
                {:else}
                  {category.tools.length}
                {/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>

      <div class="min-h-0 min-w-0 flex-1 overflow-y-auto p-2" role="group" aria-label="Tools in module">
        {#if activeCategory}
          <p class="px-1.5 pb-2 text-[11px] text-surface-500">
            {activeCategory.label}
          </p>
          <ul class="space-y-1">
            {#each activeCategory.tools as tool (tool.id)}
              {@const on = selected.includes(tool.id)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-start gap-2.5 rounded-lg px-2.5 py-2 text-left transition {on
                    ? 'bg-primary-500/10 ring-1 ring-inset ring-primary-500/30'
                    : 'hover:bg-surface-800/60'}"
                  onclick={() => onToggle(tool.id, !on)}
                  aria-pressed={on}
                >
                  <span
                    class="mt-0.5 flex size-3.5 shrink-0 items-center justify-center rounded border {on
                      ? 'border-primary-400 bg-primary-500/80 text-[9px] text-surface-950'
                      : 'border-surface-500/50'}"
                    aria-hidden="true"
                  >
                    {#if on}✓{/if}
                  </span>
                  <span class="min-w-0">
                    <span class="block text-[12px] font-medium text-surface-100"
                      >{tool.actionLabel}</span
                    >
                    <span class="mt-0.5 block truncate font-mono text-[10px] text-surface-500"
                      >{tool.id}</span
                    >
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}
</div>
