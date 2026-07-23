<script lang="ts">
  import ContextModeBar from "$lib/components/context/ContextModeBar.svelte";
  import ContextPostureList from "$lib/components/context/ContextPostureList.svelte";
  import ContextRecallList from "$lib/components/context/ContextRecallList.svelte";
  import ContextThreadsList from "$lib/components/context/ContextThreadsList.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextPosture } from "$lib/stores/contextPosture.svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import { identity } from "$lib/stores/identity.svelte";
  import {
    buildContextPostureEntries,
    filterContextPostureEntries,
  } from "$lib/utils/contextPosture";
  import {
    buildContextRecallEntries,
    filterContextRecallEntries,
  } from "$lib/utils/contextRecall";
  import {
    buildContextThreadEntries,
    filterContextThreadEntries,
  } from "$lib/utils/contextThreads";
  import { Map as MapIcon, X } from "@lucide/svelte";

  interface Props {
    /** Fired after a recall/thread/posture pick (e.g. open context from a rail popover). */
    onPick?: () => void;
    /** `rail-list` hides the mode bar (it lives in the rail popover strip). */
    chrome?: "default" | "rail-list";
  }

  let { onPick, chrome = "default" }: Props = $props();

  const showModeBar = $derived(chrome !== "rail-list");

  const activeTab = $derived(contextShell.activeTab);
  const search = $derived(contextShell.search);

  const sessionLabels = $derived(
    Object.fromEntries(
      chat.sessions.map((session) => [
        session.session_id,
        session.display_name?.trim() || session.session_id,
      ]),
    ),
  );

  const recallEntries = $derived(
    identity.context ? buildContextRecallEntries(identity.context) : [],
  );
  const filteredRecallEntries = $derived(
    filterContextRecallEntries(recallEntries, search),
  );

  const threadEntries = $derived(
    buildContextThreadEntries(contextThreads.nodes, sessionLabels),
  );
  const sessionScopedThreadEntries = $derived(
    contextShell.threadSessionFilter
      ? threadEntries.filter((entry) => entry.sessionId === contextShell.threadSessionFilter)
      : threadEntries,
  );
  const filteredThreadEntries = $derived(
    filterContextThreadEntries(sessionScopedThreadEntries, search),
  );

  const postureEntries = $derived(
    buildContextPostureEntries(contextPosture.nodes, sessionLabels),
  );
  const filteredPostureEntries = $derived(
    filterContextPostureEntries(postureEntries, search),
  );

  const searchPlaceholder = $derived.by(() => {
    if (activeTab === "map") return "Search sessions and moments…";
    if (activeTab === "threads") return "Search session moments…";
    if (activeTab === "posture") return "Search sessions or mood…";
    return "Search what she remembers…";
  });
</script>

<div
  class="context-side-panel flex h-full min-h-0 w-full flex-col"
  data-debug-label="context-side-panel"
>
  {#if showModeBar}
    <ContextModeBar />
  {/if}

  <div class="shrink-0 space-y-1.5 border-b border-surface-500/25 px-1.5 py-1.5">
    <label class="block">
      <span class="sr-only">{searchPlaceholder}</span>
      <input
        class="input w-full text-sm"
        type="search"
        placeholder={searchPlaceholder}
        value={contextShell.search}
        oninput={(event) => {
          contextShell.search = (event.currentTarget as HTMLInputElement).value;
        }}
      />
    </label>

    {#if activeTab === "threads" && contextShell.threadSessionFilter}
      <div class="flex flex-wrap items-center gap-1.5 px-0.5">
        <span class="workshop-faint text-[11px]">Session</span>
        <button
          type="button"
          class="vault-filter-chip vault-filter-chip-active inline-flex items-center gap-1"
          onclick={() => contextShell.clearThreadSessionFilter()}
        >
          <span class="max-w-[10rem] truncate">
            {sessionLabels[contextShell.threadSessionFilter] ?? contextShell.threadSessionFilter}
          </span>
          <X size={12} strokeWidth={2} />
        </button>
      </div>
    {/if}
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-2">
    {#if activeTab === "recall"}
      <ContextRecallList
        {search}
        entries={filteredRecallEntries}
        selectedId={contextShell.selectedRecallId}
        loading={identity.loading}
        error={identity.error}
        onSelect={(id) => {
          contextShell.selectRecall(id);
          onPick?.();
        }}
      />
    {:else if activeTab === "threads"}
      <ContextThreadsList
        {search}
        entries={filteredThreadEntries}
        selectedId={contextShell.selectedThreadId}
        loading={contextThreads.loading}
        error={contextThreads.error}
        retrieved={contextThreads.nodes.length}
        onSelect={(id) => {
          contextShell.selectThread(id);
          onPick?.();
        }}
      />
    {:else if activeTab === "posture"}
      <ContextPostureList
        {search}
        entries={filteredPostureEntries}
        selectedId={contextShell.selectedPostureId}
        loading={contextPosture.loading}
        error={contextPosture.error}
        onSelect={(id) => {
          contextShell.selectPosture(id);
          onPick?.();
        }}
      />
    {:else}
      <div class="flex flex-col items-center gap-2 px-3 py-8 text-center">
        <span
          class="inline-flex size-9 items-center justify-center rounded-lg bg-surface-800/70 text-surface-400"
          aria-hidden="true"
        >
          <MapIcon size={16} strokeWidth={1.75} />
        </span>
        <p class="text-sm font-medium text-surface-200">Map in the main view</p>
        <p class="workshop-faint text-xs leading-relaxed">
          Search filters the graph beside this rail.
        </p>
      </div>
    {/if}
  </div>
</div>
