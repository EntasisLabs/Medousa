<script lang="ts">
  import ContextMapMomentDetail from "$lib/components/context/ContextMapMomentDetail.svelte";
  import { listVaultNotes } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextPosture } from "$lib/stores/contextPosture.svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import type { VaultNote } from "$lib/types/vault";
  import { hasKnownChatSession } from "$lib/utils/contextCrossLinks";
  import { buildContextPostureEntries } from "$lib/utils/contextPosture";
  import {
    buildContextMapGraph,
    defaultExpandedSessionIds,
    findMapNode,
    mapNeighborhood,
    neighborSummary,
    type ContextMapNode,
  } from "$lib/utils/contextMap";
  import {
    noteMapId,
    notePathFromMapId,
    sessionIdForNoteChatTag,
  } from "$lib/utils/contextMapNotes";
  import { isWorkshopVaultTag } from "$lib/utils/vaultFrontmatter";
  import { FileText, Map as MapIcon, Square, X } from "@lucide/svelte";

  interface Props {
    /** Fired after a pick that should open the Map center surface. */
    onPick?: () => void;
  }

  let { onPick }: Props = $props();

  const search = $derived(contextShell.search);
  const selectedMapNodeId = $derived(contextShell.selectedMapNodeId);

  let vaultNotes = $state<VaultNote[]>([]);

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
  const selectedNotePath = $derived(
    selectedMapNodeId ? notePathFromMapId(selectedMapNodeId) : null,
  );
  const selectedNote = $derived(
    selectedNotePath
      ? (vaultNotes.find((note) => note.path === selectedNotePath) ?? null)
      : null,
  );
  const noteHumanTags = $derived(
    (selectedNote?.tags ?? []).filter((tag) => !isWorkshopVaultTag(tag)),
  );
  const noteLinkedSessionId = $derived(
    selectedNote
      ? sessionIdForNoteChatTag(selectedNote.tags, chatSessionIds)
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
    return noteLinkedSessionId;
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

  const railGraph = $derived.by(() => {
    const expanded = defaultExpandedSessionIds(contextThreads.nodes);
    if (mapSessionId) expanded.add(mapSessionId);
    return buildContextMapGraph(contextThreads.nodes, sessionLabels, {
      width: 320,
      height: 480,
      expandedSessionIds: expanded,
      searchQuery: search,
      density: "rail",
      vaultNotes,
    });
  });

  const selectionSummary = $derived(
    selectedMapNodeId ? neighborSummary(railGraph, selectedMapNodeId) : "",
  );

  type NearbyRow = {
    id: string;
    kind: ContextMapNode["kind"];
    label: string;
  };

  const nearbyRows = $derived.by((): NearbyRow[] => {
    if (!selectedMapNodeId) return [];
    const neighborhood = mapNeighborhood(railGraph, selectedMapNodeId);
    const rows: NearbyRow[] = [];
    for (const id of neighborhood) {
      if (id === selectedMapNodeId) continue;
      const node = findMapNode(railGraph, id);
      if (!node?.visible) continue;
      rows.push({ id: node.id, kind: node.kind, label: node.label });
    }
    // Prefer sessions, then notes, then moments for a stable scan order.
    const rank = (kind: ContextMapNode["kind"]) =>
      kind === "session" ? 0 : kind === "note" ? 1 : 2;
    return rows.sort((a, b) => rank(a.kind) - rank(b.kind)).slice(0, 8);
  });

  type BrowseRow = {
    id: string;
    kind: "session" | "note";
    label: string;
    meta: string;
  };

  const idleBrowseRows = $derived.by((): BrowseRow[] => {
    if (selectedMapNodeId) return [];

    const latestBySession = new Map<string, number>();
    for (const node of contextThreads.nodes) {
      const ts = Date.parse(node.timestamp);
      const ms = Number.isNaN(ts) ? 0 : ts;
      const prev = latestBySession.get(node.session_id) ?? 0;
      if (ms >= prev) latestBySession.set(node.session_id, ms);
    }

    const sessions: BrowseRow[] = [...latestBySession.entries()]
      .sort((left, right) => right[1] - left[1])
      .slice(0, 8)
      .map(([sessionId]) => ({
        id: `session:${sessionId}`,
        kind: "session" as const,
        label: sessionLabels[sessionId] ?? sessionId,
        meta: "Session",
      }));

    const notes: BrowseRow[] = [...vaultNotes]
      .sort(
        (left, right) =>
          Date.parse(right.modified_at_utc || "") - Date.parse(left.modified_at_utc || ""),
      )
      .slice(0, 6)
      .map((note) => ({
        id: noteMapId(note.path),
        kind: "note" as const,
        label: note.title || note.path,
        meta: note.path,
      }));

    return [...sessions, ...notes];
  });

  $effect(() => {
    void contextThreads.refresh({ limit: 200 });
    void chat.refreshSessions();
    void contextPosture.refresh();
    void (async () => {
      try {
        const response = await listVaultNotes({ limit: 200 });
        vaultNotes = response.notes;
      } catch {
        vaultNotes = [];
      }
    })();
  });

  $effect(() => {
    if (!mapThreadSyncKey) return;
    void contextThreads.loadDetail(mapThreadSyncKey);
  });

  function clearFocus() {
    contextShell.selectMapNode(null);
    contextThreads.clearDetail();
  }

  function focusMapNodeId(nodeId: string) {
    contextShell.selectMapNode(nodeId);
    if (nodeId.startsWith("thread:")) {
      void contextThreads.loadDetail(nodeId.slice("thread:".length));
    } else {
      contextThreads.clearDetail();
    }
    onPick?.();
  }

  async function openChatForSession(sessionId: string) {
    shellTabs.openChat(sessionId, { activate: true });
    await chat.switchSession(sessionId);
  }

  function openPostureForSession(sessionId: string) {
    contextShell.openPostureForSession(sessionId);
    onPick?.();
  }

  async function openSelectedNote() {
    if (!selectedNotePath) return;
    shellTabs.openSurface("library", { activate: true });
    await lmeWorkspace.openNote(selectedNotePath);
  }

  function expandSelectedSession() {
    if (!mapSessionId) return;
    contextShell.requestExpandMapSession(mapSessionId);
    onPick?.();
  }

  function kindIcon(kind: ContextMapNode["kind"] | "session" | "note") {
    if (kind === "note") return FileText;
    if (kind === "thread") return Square;
    return MapIcon;
  }
</script>

<div class="map-side-panel flex h-full min-h-0 w-full flex-col" data-debug-label="map-side-panel">
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
    {:else if selectedNote}
      <div class="space-y-3 px-0.5">
        <div class="flex items-start justify-between gap-2">
          <div class="flex min-w-0 items-start gap-2.5">
            <span
              class="inline-flex size-8 shrink-0 items-center justify-center rounded-lg bg-surface-800/70 text-surface-300"
              aria-hidden="true"
            >
              <FileText size={15} strokeWidth={1.75} />
            </span>
            <div class="min-w-0">
              <h3 class="text-sm font-semibold text-surface-100">
                {selectedNote.title || selectedNote.path}
              </h3>
              <p class="workshop-faint mt-1 break-all text-[11px] leading-relaxed">
                {selectedNote.path}
              </p>
            </div>
          </div>
          <button
            type="button"
            class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-surface-500 transition hover:bg-surface-800/80 hover:text-surface-200"
            onclick={clearFocus}
          >
            Clear
            <X size={12} strokeWidth={2} />
          </button>
        </div>

        {#if noteHumanTags.length > 0}
          <div class="flex flex-wrap gap-1">
            {#each noteHumanTags.slice(0, 8) as tag (tag)}
              <span class="vault-filter-chip text-[10px]">{tag}</span>
            {/each}
          </div>
        {/if}

        {#if selectedNote.wikilinks_out.length > 0 || selectedNote.backlinks.length > 0}
          <p class="workshop-faint text-[11px] leading-relaxed">
            {#if selectedNote.wikilinks_out.length > 0}
              {selectedNote.wikilinks_out.length} outgoing link{selectedNote.wikilinks_out
                .length === 1
                ? ""
                : "s"}{/if}{#if selectedNote.wikilinks_out.length > 0 && selectedNote.backlinks.length > 0}
              ·
            {/if}{#if selectedNote.backlinks.length > 0}
              {selectedNote.backlinks.length} backlink{selectedNote.backlinks.length === 1
                ? ""
                : "s"}{/if}
          </p>
        {:else if selectionSummary}
          <p class="workshop-faint text-[11px] leading-relaxed">{selectionSummary}</p>
        {/if}

        {#if noteLinkedSessionId}
          <p class="workshop-faint text-[11px] leading-relaxed">
            Linked chat · {sessionLabels[noteLinkedSessionId] ?? noteLinkedSessionId}
          </p>
        {/if}

        <div class="flex flex-wrap gap-2">
          <button type="button" class="btn btn-sm variant-soft-surface" onclick={openSelectedNote}>
            Open note
          </button>
          {#if noteLinkedSessionId && hasKnownChatSession(noteLinkedSessionId, chatSessionIds)}
            <button
              type="button"
              class="btn btn-sm variant-soft-surface"
              onclick={() => openChatForSession(noteLinkedSessionId)}
            >
              Open chat
            </button>
          {/if}
        </div>
      </div>
    {:else if mapSessionId && selectedMapNodeId?.startsWith("session:")}
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
                {selectionSummary || "Session on the map"}
              </p>
            </div>
          </div>
          <button
            type="button"
            class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-surface-500 transition hover:bg-surface-800/80 hover:text-surface-200"
            onclick={clearFocus}
          >
            Clear
            <X size={12} strokeWidth={2} />
          </button>
        </div>
        <div class="flex flex-wrap gap-2">
          {#if mapSessionId && mapThreadChatAvailable}
            <button
              type="button"
              class="btn btn-sm variant-soft-surface"
              onclick={() => openChatForSession(mapSessionId)}
            >
              Open chat
            </button>
          {/if}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            onclick={expandSelectedSession}
          >
            Expand moments
          </button>
        </div>
      </div>
    {:else}
      <div class="space-y-2 px-0.5">
        <p class="workshop-faint px-0.5 text-[11px] uppercase tracking-[0.08em]">On the map</p>
        {#if idleBrowseRows.length === 0}
          <p class="workshop-muted px-0.5 text-xs leading-relaxed">
            {#if contextThreads.loading && contextThreads.nodes.length === 0}
              Loading link map…
            {:else}
              Sessions and notes show up here as they accumulate.
            {/if}
          </p>
        {:else}
          <ul class="space-y-0.5">
            {#each idleBrowseRows as row (row.id)}
              {@const Icon = kindIcon(row.kind)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-start gap-2 rounded-md px-1.5 py-1.5 text-left transition hover:bg-surface-800/70"
                  onclick={() => focusMapNodeId(row.id)}
                >
                  <span
                    class="mt-0.5 inline-flex size-6 shrink-0 items-center justify-center text-surface-400"
                    aria-hidden="true"
                  >
                    <Icon size={13} strokeWidth={1.75} />
                  </span>
                  <span class="min-w-0">
                    <span class="block truncate text-[12px] text-surface-100">{row.label}</span>
                    <span class="workshop-faint block truncate text-[10px]">{row.meta}</span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    {#if selectedMapNodeId && nearbyRows.length > 0}
      <div class="mt-5 space-y-2 border-t border-surface-500/20 px-0.5 pt-3">
        <p class="workshop-faint text-[11px] uppercase tracking-[0.08em]">Nearby</p>
        <ul class="space-y-0.5">
          {#each nearbyRows as row (row.id)}
            {@const Icon = kindIcon(row.kind)}
            <li>
              <button
                type="button"
                class="flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left transition hover:bg-surface-800/70"
                onclick={() => focusMapNodeId(row.id)}
              >
                <span
                  class="inline-flex size-6 shrink-0 items-center justify-center text-surface-400"
                  aria-hidden="true"
                >
                  <Icon size={13} strokeWidth={1.75} />
                </span>
                <span class="min-w-0 truncate text-[12px] text-surface-200">{row.label}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>
</div>
