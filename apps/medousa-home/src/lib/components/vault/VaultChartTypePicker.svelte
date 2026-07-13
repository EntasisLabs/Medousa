<script lang="ts">
  import { tick } from "svelte";
  import { X } from "@lucide/svelte";
  import {
    CHART_FENCE_TYPE_OPTIONS,
    chartFenceTemplateForType,
    type ChartFenceType,
  } from "$lib/utils/liquidFenceTemplates";

  interface Props {
    open: boolean;
    onInsert: (markdown: string) => void;
    onClose: () => void;
  }

  let { open, onInsert, onClose }: Props = $props();

  let type = $state<ChartFenceType>("bar");

  $effect(() => {
    if (!open) return;
    type = "bar";
    void tick().then(() => {
      (
        document.querySelector("[data-chart-type-commit]") as HTMLButtonElement | null
      )?.focus();
    });
  });

  function commit() {
    onInsert(chartFenceTemplateForType(type));
    onClose();
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="chart-type-picker-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <form
      class="vault-interact-sheet vault-compose-sheet vault-bridge-sheet"
      onsubmit={(event) => {
        event.preventDefault();
        commit();
      }}
    >
      <header class="vault-interact-header vault-compose-header">
        <h3 id="chart-type-picker-title" class="sr-only">Insert chart</h3>
        <button
          type="button"
          class="vault-interact-dismiss ml-auto"
          aria-label="Close"
          onclick={onClose}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <p class="vault-compose-sentence">
        Insert a
        <span class="vault-compose-em">
          {CHART_FENCE_TYPE_OPTIONS.find((row) => row.id === type)?.label ?? "Bar"}
        </span>
        chart
      </p>

      <div class="vault-chip-row" role="listbox" aria-label="Chart type">
        {#each CHART_FENCE_TYPE_OPTIONS as option (option.id)}
          <button
            type="button"
            class="vault-chip"
            class:vault-chip--active={type === option.id}
            role="option"
            aria-selected={type === option.id}
            onclick={() => (type = option.id)}
          >
            {option.label}
          </button>
        {/each}
      </div>

      <div class="vault-compose-footer">
        <button type="submit" class="vault-interact-commit" data-chart-type-commit>
          Insert chart
        </button>
      </div>
    </form>
  </div>
{/if}
