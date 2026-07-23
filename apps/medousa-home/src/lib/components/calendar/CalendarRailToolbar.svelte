<script lang="ts">
  import {
    CalendarDays,
    ChevronLeft,
    ChevronRight,
    Download,
    Plus,
    RefreshCw,
    Upload,
  } from "@lucide/svelte";
  import { calendar, type CalendarViewMode } from "$lib/stores/calendar.svelte";

  interface Props {
    onAction?: () => void;
  }

  let { onAction }: Props = $props();
  let importInput: HTMLInputElement | undefined = $state();

  const modes: { id: CalendarViewMode; label: string }[] = [
    { id: "day", label: "D" },
    { id: "week", label: "W" },
    { id: "month", label: "M" },
  ];

  function newEvent() {
    calendar.openCreate();
    onAction?.();
  }

  function goToday() {
    calendar.goToday();
    onAction?.();
  }

  function setMode(mode: CalendarViewMode) {
    calendar.setViewMode(mode);
    onAction?.();
  }

  function shift(delta: number) {
    calendar.shift(delta);
    onAction?.();
  }

  async function onImportChange(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    const text = await file.text();
    await calendar.importIcs(text);
    onAction?.();
  }

  async function exportIcs() {
    const ics = await calendar.exportIcs();
    const blob = new Blob([ics], { type: "text/calendar" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "calendar.ics";
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<input
  bind:this={importInput}
  type="file"
  accept=".ics,text/calendar"
  class="sr-only"
  onchange={(event) => void onImportChange(event)}
/>

<!-- Seed rule of 3: New · Today · expand chevron (chevron lives on the popover). -->
<button
  type="button"
  class="vault-dock-icon-btn"
  title="New event"
  aria-label="New event"
  onclick={newEvent}
>
  <Plus size={15} strokeWidth={1.75} />
</button>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Today"
  aria-label="Today"
  onclick={(event) => {
    event.preventDefault();
    event.stopPropagation();
    goToday();
  }}
>
  <CalendarDays size={15} strokeWidth={1.75} />
</button>

<!-- Expanded chrome only (hidden in seed via .lme-dock-chrome-secondary). -->
<div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Previous"
    aria-label="Previous period"
    onclick={() => shift(-1)}
  >
    <ChevronLeft size={15} strokeWidth={2} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Next"
    aria-label="Next period"
    onclick={() => shift(1)}
  >
    <ChevronRight size={15} strokeWidth={2} />
  </button>
  <div
    class="inline-flex h-7 items-center rounded-md border border-surface-600/40 p-px"
    role="group"
    aria-label="View mode"
  >
    {#each modes as mode (mode.id)}
      <button
        type="button"
        class="inline-flex h-6 min-w-6 items-center justify-center rounded px-1 text-[10px] font-semibold tracking-wide transition {calendar.viewMode ===
        mode.id
          ? 'bg-surface-700 text-surface-50'
          : 'text-surface-500 hover:text-surface-200'}"
        aria-pressed={calendar.viewMode === mode.id}
        title={mode.id}
        onclick={() => setMode(mode.id)}
      >
        {mode.label}
      </button>
    {/each}
  </div>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Import .ics"
    aria-label="Import calendar"
    onclick={() => importInput?.click()}
  >
    <Upload size={14} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Export .ics"
    aria-label="Export calendar"
    onclick={() => void exportIcs()}
  >
    <Download size={14} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Refresh"
    aria-label="Refresh calendar"
    disabled={calendar.loading}
    onclick={() => void calendar.refresh()}
  >
    <RefreshCw
      size={15}
      strokeWidth={1.75}
      class={calendar.loading ? "animate-spin" : ""}
    />
  </button>
</div>
