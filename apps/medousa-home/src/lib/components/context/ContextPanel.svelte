<script lang="ts">
  import ContextMapMomentDetail from "$lib/components/context/ContextMapMomentDetail.svelte";
  import ContextMapView from "$lib/components/context/ContextMapView.svelte";
  import ContextPostureDetail from "$lib/components/context/ContextPostureDetail.svelte";
  import ContextPostureList from "$lib/components/context/ContextPostureList.svelte";
  import ContextRecallDetail from "$lib/components/context/ContextRecallDetail.svelte";
  import ContextRecallList from "$lib/components/context/ContextRecallList.svelte";
  import ContextThreadsDetail from "$lib/components/context/ContextThreadsDetail.svelte";
  import ContextThreadsList from "$lib/components/context/ContextThreadsList.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextPosture } from "$lib/stores/contextPosture.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import { identity } from "$lib/stores/identity.svelte";
  import { CONTEXT_TABS, type ContextTabId } from "$lib/types/context";
  import {
    buildContextRecallEntries,
    filterContextRecallEntries,
  } from "$lib/utils/contextRecall";
  import {
    buildContextPostureEntries,
    filterContextPostureEntries,
  } from "$lib/utils/contextPosture";
  import {
    buildContextThreadEntries,
    filterContextThreadEntries,
  } from "$lib/utils/contextThreads";
  import {
    findRelatedThreadsForClaim,
    hasKnownChatSession,
  } from "$lib/utils/contextCrossLinks";
  import type { ContextMapNode } from "$lib/utils/contextMap";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { ChevronLeft, X } from "@lucide/svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    onOpenChat?: (sessionId: string) => void | Promise<void>;
  }

  let { visible, mobile = false, embedded = false, onOpenChat }: Props = $props();

  let activeTab = $state<ContextTabId>("recall");
  let search = $state("");
  let threadSessionFilter = $state<string | null>(null);
  let selectedRecallId = $state<string | null>(null);
  let selectedThreadId = $state<string | null>(null);
  let selectedPostureId = $state<string | null>(null);
  let selectedMapNodeId = $state<string | null>(null);
  let mobileDetailOpen = $state(false);

  const sessionLabels = $derived(
    Object.fromEntries(
      chat.sessions.map((session) => [
        session.session_id,
        session.display_name?.trim() || session.session_id,
      ]),
    ),
  );
  const chatSessionIds = $derived(new Set(chat.sessions.map((session) => session.session_id)));

  const recallEntries = $derived(
    identity.context ? buildContextRecallEntries(identity.context) : [],
  );
  const filteredRecallEntries = $derived(
    filterContextRecallEntries(recallEntries, search),
  );
  const selectedRecallEntry = $derived(
    selectedRecallId
      ? (filteredRecallEntries.find((entry) => entry.id === selectedRecallId) ??
        recallEntries.find((entry) => entry.id === selectedRecallId) ??
        null)
      : null,
  );
  const relatedThreadsForRecall = $derived(
    selectedRecallEntry?.kind === "claim"
      ? findRelatedThreadsForClaim(selectedRecallEntry, contextThreads.nodes)
      : [],
  );

  const threadEntries = $derived(
    buildContextThreadEntries(contextThreads.nodes, sessionLabels),
  );
  const sessionScopedThreadEntries = $derived(
    threadSessionFilter
      ? threadEntries.filter((entry) => entry.sessionId === threadSessionFilter)
      : threadEntries,
  );
  const filteredThreadEntries = $derived(
    filterContextThreadEntries(sessionScopedThreadEntries, search),
  );
  const selectedThreadEntry = $derived(
    selectedThreadId
      ? (filteredThreadEntries.find((entry) => entry.id === selectedThreadId) ??
        threadEntries.find((entry) => entry.id === selectedThreadId) ??
        null)
      : null,
  );

  const postureEntries = $derived(
    buildContextPostureEntries(contextPosture.nodes, sessionLabels),
  );
  const filteredPostureEntries = $derived(
    filterContextPostureEntries(postureEntries, search),
  );
  const selectedPostureEntry = $derived(
    selectedPostureId
      ? (filteredPostureEntries.find((entry) => entry.id === selectedPostureId) ??
        postureEntries.find((entry) => entry.id === selectedPostureId) ??
        null)
      : null,
  );

  const threadDetailSessionId = $derived(
    contextThreads.detail?.node.session_id ?? selectedThreadEntry?.sessionId ?? null,
  );
  const threadPostureAvailable = $derived(
    threadDetailSessionId
      ? postureEntries.some((entry) => entry.sessionId === threadDetailSessionId)
      : false,
  );
  const threadChatAvailable = $derived(
    threadDetailSessionId
      ? hasKnownChatSession(threadDetailSessionId, chatSessionIds)
      : false,
  );

  const mapThreadSyncKey = $derived(
    selectedMapNodeId?.startsWith("thread:")
      ? selectedMapNodeId.slice("thread:".length)
      : null,
  );
  const mapThreadSessionId = $derived(
    mapThreadSyncKey
      ? (contextThreads.detail?.node.session_id ??
        contextThreads.nodes.find((node) => node.sync_key === mapThreadSyncKey)?.session_id ??
        null)
      : null,
  );
  const mapThreadChatAvailable = $derived(
    mapThreadSessionId ? hasKnownChatSession(mapThreadSessionId, chatSessionIds) : false,
  );
  const mapThreadPostureAvailable = $derived(
    mapThreadSessionId
      ? postureEntries.some((entry) => entry.sessionId === mapThreadSessionId)
      : false,
  );

  const searchPlaceholder = $derived.by(() => {
    if (activeTab === "map") return "Search sessions and moments…";
    if (activeTab === "threads") return "Search session moments…";
    if (activeTab === "posture") return "Search sessions or mood…";
    return "Search what she remembers…";
  });

  const mobileDetailLabel = $derived.by(() => {
    if (activeTab === "map") return "Map detail";
    if (activeTab === "threads") return "Thread detail";
    if (activeTab === "posture") return "Posture detail";
    return "Recall detail";
  });

  const showRefresh = $derived(
    activeTab === "recall" ||
      activeTab === "threads" ||
      activeTab === "posture" ||
      activeTab === "map",
  );

  $effect(() => {
    if (!visible) return;
    const focusId = contextThreads.railFocusSyncKey;
    if (!focusId) return;
    activeTab = "threads";
    selectedThreadId = focusId;
    mobileDetailOpen = mobile;
    contextThreads.consumeRailFocus();
  });

  $effect(() => {
    if (!visible || activeTab !== "recall") return;
    void identity.refresh();
  });

  $effect(() => {
    if (!visible || activeTab !== "recall") return;
    if (contextThreads.nodes.length > 0) return;
    void contextThreads.refresh();
  });

  $effect(() => {
    if (!visible || activeTab !== "threads") return;
    void contextThreads.refresh(
      threadSessionFilter ? { sessionId: threadSessionFilter } : undefined,
    );
  });

  $effect(() => {
    if (!visible || activeTab !== "posture") return;
    void chat.refreshSessions();
    void contextPosture.refresh();
  });

  $effect(() => {
    if (!visible || activeTab !== "map") return;
    void contextThreads.refresh();
    void chat.refreshSessions();
    void contextPosture.refresh();
  });

  $effect(() => {
    if (activeTab !== "map" || !mapThreadSyncKey) return;
    void contextThreads.loadDetail(mapThreadSyncKey);
  });

  $effect(() => {
    if (activeTab !== "recall") return;
    if (filteredRecallEntries.length === 0) {
      selectedRecallId = null;
      return;
    }
    if (
      selectedRecallId &&
      filteredRecallEntries.some((entry) => entry.id === selectedRecallId)
    ) {
      return;
    }
    if (!mobile) {
      selectedRecallId = filteredRecallEntries[0]?.id ?? null;
    }
  });

  $effect(() => {
    if (activeTab !== "threads") return;
    if (filteredThreadEntries.length === 0) {
      selectedThreadId = null;
      contextThreads.clearDetail();
      return;
    }
    if (
      selectedThreadId &&
      filteredThreadEntries.some((entry) => entry.id === selectedThreadId)
    ) {
      return;
    }
    if (!mobile) {
      const first = filteredThreadEntries[0]?.id ?? null;
      selectedThreadId = first;
      if (first) {
        void contextThreads.loadDetail(first);
      }
    }
  });

  $effect(() => {
    if (activeTab !== "threads" || !selectedThreadId) return;
    void contextThreads.loadDetail(selectedThreadId);
  });

  $effect(() => {
    if (activeTab !== "posture") return;
    if (filteredPostureEntries.length === 0) {
      selectedPostureId = null;
      return;
    }
    if (
      selectedPostureId &&
      filteredPostureEntries.some((entry) => entry.id === selectedPostureId)
    ) {
      return;
    }
    if (!mobile) {
      selectedPostureId = filteredPostureEntries[0]?.id ?? null;
    }
  });

  function selectRecall(id: string) {
    selectedRecallId = id;
    if (mobile) mobileDetailOpen = true;
  }

  function selectThread(id: string) {
    selectedThreadId = id;
    if (mobile) mobileDetailOpen = true;
  }

  function selectPosture(id: string) {
    selectedPostureId = id;
    if (mobile) mobileDetailOpen = true;
  }

  function clearMapFocus() {
    selectedMapNodeId = null;
    contextThreads.clearDetail();
    mobileDetailOpen = false;
  }

  function focusMapNode(node: ContextMapNode) {
    selectedMapNodeId = node.id;
    if (node.kind === "thread" && node.syncKey) {
      void contextThreads.loadDetail(node.syncKey);
      if (mobile) mobileDetailOpen = true;
      return;
    }
    contextThreads.clearDetail();
  }

  function setTab(tab: ContextTabId) {
    const meta = CONTEXT_TABS.find((entry) => entry.id === tab);
    if (!meta?.available) return;
    activeTab = tab;
    search = "";
    selectedRecallId = null;
    selectedThreadId = null;
    selectedPostureId = null;
    selectedMapNodeId = null;
    mobileDetailOpen = false;
    if (tab !== "threads") {
      threadSessionFilter = null;
    }
    contextThreads.clearDetail();
  }

  function clearThreadSessionFilter() {
    threadSessionFilter = null;
    selectedThreadId = null;
    mobileDetailOpen = false;
    contextThreads.clearDetail();
    if (activeTab === "threads" && visible) {
      void contextThreads.refresh();
    }
  }

  function openThreadsForSession(sessionId: string, syncKey?: string) {
    const meta = CONTEXT_TABS.find((entry) => entry.id === "threads");
    if (!meta?.available) return;
    activeTab = "threads";
    threadSessionFilter = sessionId;
    search = "";
    selectedRecallId = null;
    selectedPostureId = null;
    selectedThreadId = syncKey ?? null;
    mobileDetailOpen = Boolean(syncKey && mobile);
    contextThreads.clearDetail();
    void contextThreads.refresh({ sessionId }).then(() => {
      if (syncKey) {
        void contextThreads.loadDetail(syncKey);
      }
    });
  }

  function openPostureForSession(sessionId: string) {
    const meta = CONTEXT_TABS.find((entry) => entry.id === "posture");
    if (!meta?.available) return;
    activeTab = "posture";
    search = "";
    threadSessionFilter = null;
    selectedRecallId = null;
    selectedThreadId = null;
    mobileDetailOpen = false;
    contextThreads.clearDetail();
    const entry =
      postureEntries.find((candidate) => candidate.sessionId === sessionId) ??
      filteredPostureEntries.find((candidate) => candidate.sessionId === sessionId);
    selectedPostureId = entry?.id ?? null;
    if (entry && mobile) {
      mobileDetailOpen = true;
    } else if (!entry) {
      search = sessionId;
    }
  }

  function searchThreads(query: string) {
    const meta = CONTEXT_TABS.find((entry) => entry.id === "threads");
    if (!meta?.available) return;
    activeTab = "threads";
    threadSessionFilter = null;
    search = query.trim();
    selectedRecallId = null;
    selectedPostureId = null;
    selectedThreadId = null;
    mobileDetailOpen = false;
    contextThreads.clearDetail();
    void contextThreads.refresh(search ? { q: search } : undefined);
  }

  function openChatForSession(sessionId: string) {
    if (!onOpenChat) return;
    void onOpenChat(sessionId);
  }

  function refreshActiveTab() {
    if (activeTab === "recall") {
      void identity.refresh();
      return;
    }
    if (activeTab === "threads") {
      void contextThreads.refresh();
      return;
    }
    if (activeTab === "posture") {
      void chat.refreshSessions();
      void contextPosture.refresh();
      return;
    }
    if (activeTab === "map") {
      void contextThreads.refresh();
      void chat.refreshSessions();
      void contextPosture.refresh();
    }
  }

  const refreshLoading = $derived(
    activeTab === "recall"
      ? identity.loading
      : activeTab === "threads" || activeTab === "map"
        ? contextThreads.loading
        : contextPosture.loading,
  );

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (!mobileDetailOpen) return false;
      mobileDetailOpen = false;
      return true;
    });
  });
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !mobile || !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div class="flex min-w-0 items-start gap-2">
            <ShellSidebarExpandButton label="Show rail" />
            <div class="min-w-0">
              <h1 class="text-base font-semibold text-surface-50">Context</h1>
              <p class="workshop-header-line mt-1">
                The shelf — what happened, what she remembers, how you showed up
              </p>
            </div>
          </div>
          {#if showRefresh}
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface shrink-0"
              disabled={refreshLoading}
              onclick={refreshActiveTab}
            >
              {refreshLoading ? "Refreshing…" : "Refresh"}
            </button>
          {/if}
        </div>
      {:else if showRefresh}
        <div class="flex items-center justify-end">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface shrink-0"
            disabled={refreshLoading}
            onclick={refreshActiveTab}
          >
            {refreshLoading ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      {/if}

      <div class="workshop-tabs {embedded ? 'mt-0' : 'mt-3'}">
        {#each CONTEXT_TABS as tab (tab.id)}
          <button
            type="button"
            class="workshop-tab {activeTab === tab.id ? 'workshop-tab-active' : ''} {!tab.available
              ? 'opacity-50'
              : ''}"
            disabled={!tab.available}
            title={tab.available ? tab.hint : `${tab.hint} — coming soon`}
            onclick={() => setTab(tab.id)}
          >
            {tab.label}
          </button>
        {/each}
      </div>

      {#if showRefresh && (!mobile || !mobileDetailOpen)}
        <label class="mt-3 block">
          <span class="sr-only">{searchPlaceholder}</span>
          <input
            class="input w-full {embedded ? '' : 'max-w-lg'} text-sm"
            type="search"
            placeholder={searchPlaceholder}
            bind:value={search}
          />
        </label>
      {/if}

      {#if activeTab === "threads" && threadSessionFilter && (!mobile || !mobileDetailOpen)}
        <div class="mt-2 flex flex-wrap items-center gap-2">
          <span class="workshop-faint text-[11px]">Filtered to session</span>
          <button
            type="button"
            class="vault-filter-chip vault-filter-chip-active inline-flex items-center gap-1"
            onclick={clearThreadSessionFilter}
          >
            <span class="max-w-[14rem] truncate">
              {sessionLabels[threadSessionFilter] ?? threadSessionFilter}
            </span>
            <X size={12} strokeWidth={2} />
          </button>
        </div>
      {/if}
    </header>
  {:else if mobile && mobileDetailOpen}
    <div class="flex items-center gap-2 border-b border-surface-500/40 px-4 py-2">
      <button
        type="button"
        class="mobile-icon-btn shrink-0"
        aria-label="Back to list"
        onclick={() => {
          mobileDetailOpen = false;
        }}
      >
        <ChevronLeft size={20} strokeWidth={1.75} />
      </button>
      <p class="workshop-faint truncate text-xs">{mobileDetailLabel}</p>
    </div>
  {/if}

  {#if activeTab === "recall"}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      {#if !mobile || !mobileDetailOpen}
        <aside
          class="workshop-list-pane mobile-you-scroll min-w-0 shrink-0 overflow-y-auto px-3 py-3 {mobile
            ? 'w-full'
            : 'w-[min(300px,34%)] border-r border-surface-500/40'}"
        >
          <ContextRecallList
            {search}
            entries={filteredRecallEntries}
            selectedId={selectedRecallId}
            loading={identity.loading}
            error={identity.error}
            onSelect={selectRecall}
          />
        </aside>
      {/if}

      {#if !mobile || mobileDetailOpen}
        <div
          class="workshop-detail-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4 {mobile
            ? ''
            : 'border-l border-surface-500/40'}"
        >
          <ContextRecallDetail
            entry={selectedRecallEntry}
            context={identity.context}
            {sessionLabels}
            relatedThreads={relatedThreadsForRecall}
            onOpenThread={(syncKey, sessionId) => openThreadsForSession(sessionId, syncKey)}
            onSearchThreads={searchThreads}
          />
        </div>
      {/if}
    </div>
  {:else if activeTab === "threads"}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      {#if !mobile || !mobileDetailOpen}
        <aside
          class="workshop-list-pane mobile-you-scroll min-w-0 shrink-0 overflow-y-auto px-3 py-3 {mobile
            ? 'w-full'
            : 'w-[min(300px,34%)] border-r border-surface-500/40'}"
        >
          <ContextThreadsList
            {search}
            entries={filteredThreadEntries}
            selectedId={selectedThreadId}
            loading={contextThreads.loading}
            error={contextThreads.error}
            retrieved={contextThreads.nodes.length}
            onSelect={selectThread}
          />
        </aside>
      {/if}

      {#if !mobile || mobileDetailOpen}
        <div
          class="workshop-detail-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4 {mobile
            ? ''
            : 'border-l border-surface-500/40'}"
        >
          <ContextThreadsDetail
            detail={contextThreads.detail}
            loading={contextThreads.detailLoading}
            error={contextThreads.detailError}
            {sessionLabels}
            chatSessionAvailable={threadChatAvailable}
            postureAvailable={threadPostureAvailable}
            onOpenChat={
              threadDetailSessionId && onOpenChat
                ? () => openChatForSession(threadDetailSessionId)
                : undefined
            }
            onOpenPosture={
              threadDetailSessionId
                ? () => openPostureForSession(threadDetailSessionId)
                : undefined
            }
          />
        </div>
      {/if}
    </div>
  {:else if activeTab === "posture"}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      {#if !mobile || !mobileDetailOpen}
        <aside
          class="workshop-list-pane mobile-you-scroll min-w-0 shrink-0 overflow-y-auto px-3 py-3 {mobile
            ? 'w-full'
            : 'w-[min(300px,34%)] border-r border-surface-500/40'}"
        >
          <ContextPostureList
            {search}
            entries={filteredPostureEntries}
            selectedId={selectedPostureId}
            loading={contextPosture.loading}
            error={contextPosture.error}
            onSelect={selectPosture}
          />
        </aside>
      {/if}

      {#if !mobile || mobileDetailOpen}
        <div
          class="workshop-detail-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4 {mobile
            ? ''
            : 'border-l border-surface-500/40'}"
        >
          <ContextPostureDetail
            entry={selectedPostureEntry}
            chatSessionAvailable={selectedPostureEntry
              ? hasKnownChatSession(selectedPostureEntry.sessionId, chatSessionIds)
              : false}
            onOpenChat={
              selectedPostureEntry && onOpenChat
                ? () => openChatForSession(selectedPostureEntry.sessionId)
                : undefined
            }
            onOpenThreads={
              selectedPostureEntry
                ? () => openThreadsForSession(selectedPostureEntry.sessionId)
                : undefined
            }
            onOpenLatestThread={
              selectedPostureEntry?.latestSyncKey
                ? () =>
                    openThreadsForSession(
                      selectedPostureEntry.sessionId,
                      selectedPostureEntry.latestSyncKey,
                    )
                : undefined
            }
          />
        </div>
      {/if}
    </div>
  {:else if activeTab === "map"}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      {#if !mobile || !mobileDetailOpen}
        <div
          class="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden {mobile
            ? 'w-full'
            : mapThreadSyncKey
              ? 'border-r border-surface-500/40'
              : ''}"
        >
          <ContextMapView
            nodes={contextThreads.nodes}
            {sessionLabels}
            {search}
            loading={contextThreads.loading}
            error={contextThreads.error}
            selectedNodeId={selectedMapNodeId}
            onFocusNode={focusMapNode}
            onClearSelection={clearMapFocus}
          />
        </div>
      {/if}

      {#if mobile && mobileDetailOpen && mapThreadSyncKey}
        <div
          class="workshop-detail-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4"
        >
          <ContextMapMomentDetail
            detail={contextThreads.detail}
            loading={contextThreads.detailLoading}
            error={contextThreads.detailError}
            chatSessionAvailable={mapThreadChatAvailable}
            postureAvailable={mapThreadPostureAvailable}
            onOpenChat={
              mapThreadSessionId && onOpenChat
                ? () => openChatForSession(mapThreadSessionId)
                : undefined
            }
            onOpenPosture={
              mapThreadSessionId
                ? () => openPostureForSession(mapThreadSessionId)
                : undefined
            }
          />
        </div>
      {:else if !mobile && mapThreadSyncKey}
        <div
          class="workshop-detail-pane mobile-you-scroll min-w-0 w-[min(380px,38%)] shrink-0 overflow-y-auto border-l border-surface-500/40 px-4 py-4"
        >
          <ContextMapMomentDetail
            detail={contextThreads.detail}
            loading={contextThreads.detailLoading}
            error={contextThreads.detailError}
            chatSessionAvailable={mapThreadChatAvailable}
            postureAvailable={mapThreadPostureAvailable}
            onOpenChat={
              mapThreadSessionId && onOpenChat
                ? () => openChatForSession(mapThreadSessionId)
                : undefined
            }
            onOpenPosture={
              mapThreadSessionId
                ? () => openPostureForSession(mapThreadSessionId)
                : undefined
            }
          />
        </div>
      {/if}
    </div>
  {/if}
</section>
