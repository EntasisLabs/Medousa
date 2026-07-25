<script lang="ts">
  import ContextMapMomentDetail from "$lib/components/context/ContextMapMomentDetail.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextPosture } from "$lib/stores/contextPosture.svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { hasKnownChatSession } from "$lib/utils/contextCrossLinks";
  import { buildContextPostureEntries } from "$lib/utils/contextPosture";
  import { Map as MapIcon, X } from "@lucide/svelte";

  interface Props {
    /** Fired after a pick that should open the Map center surface. */
    onPick?: () => void;
  }

  let { onPick }: Props = $props();

  const search = $derived(contextShell.search);
  const selectedMapNodeId = $derived(contextShell.selectedMapNodeId);

  const sessionLabels = $derived(
    Object.fromEntries(
      chat.sessions.map((session) => [
        session.session_id,
        session.display_name?.trim() || session.session_id,
      ]),
    ),
  );
  const chatSessionIds = $derived(new Set(chat.sessions.map((session) => session.session_id)));
  const postureEntries = $derived(
    buildContextPostureEntries(contextPosture.nodes, sessionLabels),
  );

  const mapThreadSyncKey = $derived(
    selectedMapNodeId?.startsWith("thread:")
      ? selectedMapNodeId.slice("thread:".length)
      : null,
  );
  const mapSessionId = $derived.by(() => {
    if (mapThreadSyncKey) {
      return (
        contextThreads.detail?.node.session_id ??
        contextThreads.nodes.find((node) => node.sync_key === mapThreadSyncKey)?.session_id ??
        null
      );
    }
    if (selectedMapNodeId?.startsWith("session:")) {
      return selectedMapNodeId.slice("session:".length);
    }
    return null;
  });
  const mapThreadChatAvailable = $derived(
    mapSessionId ? hasKnownChatSession(mapSessionId, chatSessionIds) : false,
  );
  const mapThreadPostureAvailable = $derived(
    mapSessionId
      ? postureEntries.some((entry) => entry.sessionId === mapSessionId)
      : false,
  );
  const sessionLabel = $derived(
    mapSessionId ? (sessionLabels[mapSessionId] ?? mapSessionId) : null,
  );
  const momentCount = $derived(new Set(contextThreads.nodes.map((node) => node.sync_key)).size);
  const sessionCount = $derived(
    new Set(contextThreads.nodes.map((node) => node.session_id)).size,
  );

  $effect(() => {
    void contextThreads.refresh({ limit: 200 });
    void chat.refreshSessions();
    void contextPosture.refresh();
  });

  $effect(() => {
    if (!mapThreadSyncKey) return;
    void contextThreads.loadDetail(mapThreadSyncKey);
  });

  function clearFocus() {
    contextShell.selectMapNode(null);
    contextThreads.clearDetail();
  }

  async function openChatForSession(sessionId: string) {
    shellTabs.openChat(sessionId, { activate: true });
    await chat.switchSession(sessionId);
  }

  function openPostureForSession(sessionId: string) {
    contextShell.openPostureForSession(sessionId);
    onPick?.();
  }
</script>

<div class="map-side-panel flex h-full min-h-0 w-full flex-col" data-debug-label="map-side-panel">
  <div class="shrink-0 space-y-1.5 border-b border-surface-500/25 px-1.5 py-1.5">
    <label class="block">
      <span class="sr-only">Search sessions and moments</span>
      <input
        class="input w-full text-sm"
        type="search"
        placeholder="Search sessions and moments…"
        value={search}
        oninput={(event) => {
          contextShell.search = (event.currentTarget as HTMLInputElement).value;
          onPick?.();
        }}
      />
    </label>
    <p class="workshop-faint px-0.5 text-[11px] leading-relaxed">
      {#if contextThreads.loading && contextThreads.nodes.length === 0}
        Loading link map…
      {:else}
        {sessionCount} session{sessionCount === 1 ? "" : "s"} · {momentCount} moment{momentCount === 1
          ? ""
          : "s"}
      {/if}
    </p>
  </div>

  <div class="workshop-detail-pane mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-2 py-3">
    {#if mapThreadSyncKey}
      <div class="mb-2 flex items-center justify-end px-0.5">
        <button
          type="button"
          class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-surface-500 transition hover:bg-surface-800/80 hover:text-surface-200"
          onclick={clearFocus}
        >
          Clear
          <X size={12} strokeWidth={2} />
        </button>
      </div>
      <ContextMapMomentDetail
        detail={contextThreads.detail}
        loading={contextThreads.detailLoading}
        error={contextThreads.detailError}
        chatSessionAvailable={mapThreadChatAvailable}
        postureAvailable={mapThreadPostureAvailable}
        onOpenChat={
          mapSessionId && mapThreadChatAvailable
            ? () => openChatForSession(mapSessionId)
            : undefined
        }
        onOpenPosture={
          mapSessionId ? () => openPostureForSession(mapSessionId) : undefined
        }
      />
    {:else if mapSessionId}
      <div class="space-y-3 px-0.5">
        <div class="flex items-start justify-between gap-2">
          <div class="flex min-w-0 items-start gap-2.5">
            <span
              class="inline-flex size-8 shrink-0 items-center justify-center rounded-lg bg-surface-800/70 text-surface-300"
              aria-hidden="true"
            >
              <MapIcon size={15} strokeWidth={1.75} />
            </span>
            <div class="min-w-0">
              <h3 class="text-sm font-semibold text-surface-100">{sessionLabel}</h3>
              <p class="workshop-faint mt-1 text-xs leading-relaxed">
                Double-click the session on the map to expand its moments.
              </p>
            </div>
          </div>
          <button
            type="button"
            class="inline-flex shrink-0 items-center justify-center rounded-md p-1 text-surface-500 transition hover:bg-surface-800/80 hover:text-surface-200"
            aria-label="Clear focus"
            onclick={clearFocus}
          >
            <X size={14} strokeWidth={2} />
          </button>
        </div>
        {#if mapSessionId && mapThreadChatAvailable}
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            onclick={() => openChatForSession(mapSessionId)}
          >
            Open chat
          </button>
        {/if}
      </div>
    {:else}
      <div class="flex h-full min-h-[10rem] items-center justify-center px-3">
        <p class="workshop-muted text-center text-xs leading-relaxed">
          Click a session or moment on the map. Detail lands here.
        </p>
      </div>
    {/if}
  </div>
</div>
