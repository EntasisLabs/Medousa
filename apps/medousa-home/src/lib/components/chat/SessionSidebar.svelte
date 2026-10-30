<script lang="ts">
  import "$lib/styles/chat.postcss";
  import { onMount, untrack } from "svelte";
  import { ChevronDown, ChevronRight, Plus, Search, Users, X } from "@lucide/svelte";
  import { agentCreation } from "$lib/stores/agentCreation.svelte";
  import ConnectedAgentList from "./ConnectedAgentList.svelte";
  import SidebarSectionHeading from "./SidebarSectionHeading.svelte";
  import { connection } from '$lib/stores/connection.svelte';
  import { listPeerProposals } from '$lib/daemon/coordination';
  import { sidebarRelations } from './sidebarRelations.svelte';
  import { sessionFamilies } from '$lib/utils/sessionRelations';
  import { readSidebarValue, writeSidebarValue, sidebarKey } from '$lib/utils/sidebarState';
  import { getSessionAgentRuntime, isProviderConversationRuntime } from "$lib/utils/sessionAgentRuntime";
  import { botExecutorUpdate } from "$lib/utils/botExecutorUpdate";
  import BotEditor from "$lib/components/chat/BotEditor.svelte";
  import { DEFAULT_BOT_AVATAR } from "$lib/utils/botAvatar";
  import BotRow from "$lib/components/chat/BotRow.svelte";
  import SessionRow from "$lib/components/chat/SessionRow.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { chat } from "$lib/stores/chat.svelte";
  import { loadMoreSessions } from "$lib/chat/sessionController";
  import { SessionSearch } from "$lib/chat/sessionSearch.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { sharedMode } from "$lib/stores/sharedMode.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { SessionSummary } from "$lib/types/session";
  import type { BotProfile, BotWorldBinding, ExternalAgentExecutor } from "$lib/types/generated/daemon_api";
  import { formatSessionLabel } from "$lib/utils/formatSession";
  import { groupSessionsByRecency } from "$lib/utils/sessionHistoryGroups";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    onOpenSession?: (sessionId: string, title?: string) => void | Promise<void>;
    onNewSession?: () => void | Promise<void>;
    onClose?: () => void;
    /** Fired after a session is opened/created (e.g. close a rail popover). */
    onPick?: () => void;
    variant?: "drawer" | "inline" | "sheet";
    /** `rail-list` hides the built-in toolbar (actions live in the rail popover strip). */
    chrome?: "default" | "rail-list";
  }

  let { open, onClose, onPick, onOpenSession, onNewSession, variant = "drawer", chrome = "default" }: Props = $props();

  const showBuiltInToolbar = $derived(chrome !== "rail-list");

  let query = $state("");
  const sessionSearch = new SessionSearch();
  const searchScope = $derived(`${chat.workshopScopeId}:${chat.workshopEpoch}`);
  $effect(() => {
    const scope = searchScope;
    const needle = query;
    if (!open || !chat.workshopScopeId) { untrack(() => sessionSearch.cancel()); return; }
    untrack(() => { void sessionSearch.search(scope, needle); });
    return () => sessionSearch.cancel();
  });
  const searchedSessions = $derived(query.trim()
    ? sessionSearch.resultsFor(searchScope, query, chat.sessions) : chat.sessions);
  let now = $state(Date.now());
  $effect(() => {
    if (!open) return;
    const timer = setInterval(() => { now = Date.now(); }, 60_000);
    return () => clearInterval(timer);
  });
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let renamingSession = $state<SessionSummary | null>(null);
  let renameDraft = $state("");
  let renameError = $state<string | null>(null);
  let renameSaving = $state(false);
  let deletingSession = $state<SessionSummary | null>(null);
  let deleteError = $state<string | null>(null);
  let deleteSaving = $state(false);
  let renameInputEl = $state<HTMLInputElement | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);
  let archivedBotsCollapsed = $state(true);
  let botEditorOpen = $state(false);
  let editingBot = $state<BotProfile | null>(null);
  let botName = $state("");
  let botRole = $state("");
  let botAvatar = $state<string>(DEFAULT_BOT_AVATAR);
  let botSpecialistId = $state("");
  let botExternalAgent = $state<ExternalAgentExecutor | null>(null);
  let botWorldBinding = $state<BotWorldBinding | null>(null);
  let botSaving = $state(false);
  let botError = $state<string | null>(null);
  let botActionId = $state<string | null>(null);

  const touchActions = $derived(variant === "sheet");

  $effect(() => {
    if (!open) return;
    untrack(() => {
      query = "";
      chat.sessionListQuery = "";
      void chat.refreshSessions({ force: true, q: "" });
      void bots.refresh().catch(() => undefined);
      if (catalog.manuscripts.length === 0 && !catalog.loading) {
        void catalog.refresh();
      }
      // Desktop drawers can take focus immediately; mobile sheets should open
      // without summoning the keyboard over the session list.
      if (showBuiltInToolbar && variant !== "sheet") {
        queueMicrotask(() => searchInputEl?.focus());
      }
    });
  });

  // Keep list filter in sync when rail toolbar owns the search field.
  $effect(() => {
    if (!open || chrome !== "rail-list") return;
    query = chat.sessionListQuery;
  });

  $effect(() => {
    const needle = query;
    if (!open || chrome === "rail-list") return;
    chat.sessionListQuery = needle;
  });

  function closeSheet() {
    layout.setSessionDrawerOpen(false);
    onClose?.();
  }

  function dismissSheet() {
    haptic("light");
    closeSheet();
  }

  $effect(() => {
    if (!open || variant !== "sheet") return;
    return registerMobileBackHandler(() => {
      closeSheet();
      return true;
    });
  });

  $effect(() => {
    if (!open || variant !== "sheet" || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismissSheet });
  });

  $effect(() => {
    if (renamingSession) {
      renameInputEl?.focus();
    }
  });

  function matchesQuery(session: SessionSummary): boolean {
    if (!query.trim()) return true;
    const needle = query.trim().toLowerCase();
    return [session.display_name ?? "", session.preview, session.session_id]
      .some((value) => value.toLowerCase().includes(needle));
  }

  const botSessionIds = $derived(
    new Set(
      bots.bots
        .map((bot) => bot.primary_session_id?.trim())
        .filter((sessionId): sessionId is string => Boolean(sessionId)),
    ),
  );

  const relationScope = $derived(sidebarKey(chat.workshopScopeId, connection.health?.runtime?.authority_id ?? '', connection.health?.active_profile_id ?? '', 'families'));
  const candidates = $derived(searchedSessions.filter(session => !botSessionIds.has(session.session_id)
    && !isProviderConversationRuntime(getSessionAgentRuntime(session.session_id))));
  const families = $derived(sessionFamilies(candidates, sidebarRelations.scope === relationScope ? sidebarRelations.links : [], new Set(chat.pinnedIds)));
  const visibleSessions = $derived(query.trim() ? candidates.map(families.display) : families.roots);
  let familyCollapsed = $state<Record<string, boolean>>({});
  function familyKey(id: string): string {
    return sidebarKey(chat.workshopScopeId, connection.health?.runtime?.authority_id ?? '', connection.health?.active_profile_id ?? '', 'related:' + id);
  }
  function familyExpanded(id: string): boolean {
    const key = familyKey(id);
    const initiallyCollapsed = !families.children.get(id)?.some(child => child.session_id === chat.focusedSessionId);
    return !(familyCollapsed[key] ?? readSidebarValue(key, initiallyCollapsed));
  }
  function toggleFamily(id: string) {
    const key = familyKey(id);
    const collapsed = familyExpanded(id);
    familyCollapsed[key] = collapsed;
    writeSidebarValue(key, collapsed);
  }
  const discoveryIds = $derived(JSON.stringify(searchedSessions.map(session => session.session_id).sort()));
  const relationAuthority = $derived(connection.health?.runtime?.authority_id ?? '');
  const discoveryAvailable = $derived(open && connection.online && Boolean(connection.health?.runtime?.advertised_capabilities.includes('coordination.operator_proposals.v1')));
  $effect(() => { sidebarRelations.load(relationScope); });
  $effect(() => {
    const scope = relationScope;
    const authority = relationAuthority;
    const ids = JSON.parse(discoveryIds) as string[];
    const workshopScope = chat.workshopScopeId;
    if (!discoveryAvailable) return;
    let cancelled = false;
    // Three bounded lanes; never request every transcript or guess from names.
    const queue = [...ids];
    async function discover() {
      while (!cancelled && queue.length) {
        const id = queue.shift()!;
        let after: string | undefined;
        const seen = new Set<string>();
        try {
          do {
            const page = await listPeerProposals(id, null, after, undefined, workshopScope);
            if (cancelled) return;
            sidebarRelations.observe(scope, [...page.proposals, ...(page.tracked_proposal ? [page.tracked_proposal] : [])], authority);
            after = page.next_cursor ?? undefined;
            if (after && seen.has(after)) break;
            if (after) seen.add(after);
          } while (after && !cancelled);
        } catch { /* Older/offline workshops keep their normal flat history. */ }
      }
    }
    for (let lane = 0; lane < 3; lane++) void discover();
    return () => { cancelled = true; };
  });

  // Pins are an explicit destination, independent of collapsed family roots.
  const pinned = $derived(
    candidates.map(families.display).filter(
      (session) => chat.isPinned(session.session_id) && matchesQuery(session),
    ),
  );

  const recent = $derived(
    visibleSessions.filter(
      (session) =>
        !botSessionIds.has(session.session_id) &&
        !isProviderConversationRuntime(getSessionAgentRuntime(session.session_id)) &&
        !chat.isPinned(session.session_id) &&
        matchesQuery(session),
    ),
  );
  const recentGroups = $derived(groupSessionsByRecency(recent, new Date(now)));

  const listEmpty = $derived(pinned.length === 0 && recent.length === 0);

  function matchesBotQuery(bot: BotProfile): boolean {
    if (!query.trim()) return true;
    const needle = query.trim().toLowerCase();
    const specialist = catalog.manuscripts.find(
      (entry) => entry.id === bot.primary_manuscript_id,
    );
    return [bot.display_name, bot.role_description ?? "", specialist?.name ?? ""]
      .some((value) => value.toLowerCase().includes(needle));
  }

  const activeBots = $derived(
    bots.bots.filter((bot) => !bot.archived && matchesBotQuery(bot)),
  );
  const archivedBots = $derived(
    bots.bots.filter((bot) => bot.archived && matchesBotQuery(bot)),
  );

  function toggleSessionPin(session: SessionSummary) {
    // A server search hit may never have been in the browse cache.
    if (!chat.isPinned(session.session_id) && !chat.sessions.some(row => row.session_id === session.session_id)) {
      chat.sessions = [...chat.sessions, session];
    }
    chat.togglePin(session.session_id);
  }

  async function selectSession(sessionId: string, title?: string) {
    if (onOpenSession) { await onOpenSession(sessionId, title); onPick?.(); return; }
    // Shell tabs own visible chat selection. Activating through the shell also
    // switches/hydrates the chat store, without relying on its async mirror.
    const tabId = shellTabs.openChat(sessionId, { activate: true, title });
    if (!tabId) await chat.switchSession(sessionId);
    onPick?.();
    if (variant === "drawer" || variant === "sheet") {
      layout.setSessionDrawerOpen(false);
      onClose?.();
    }
  }

  onMount(() => {
    void sharedMode.load();
  });

  async function createSession() {
    if (onNewSession) { await onNewSession(); onPick?.(); return; }
    await chat.newSession();
    onPick?.();
    if (variant === "drawer" || variant === "sheet") {
      layout.setSessionDrawerOpen(false);
      onClose?.();
    }
  }

  function specialistLabel(bot: BotProfile): string {
    return catalog.manuscripts.find((entry) => entry.id === bot.primary_manuscript_id)?.name ??
      bot.primary_manuscript_id;
  }

  async function selectBot(bot: BotProfile) {
    if (botActionId) return;
    botActionId = bot.bot_id;
    botError = null;
    try {
      const response = await bots.open(bot);
      await chat.refreshSessions({ force: true });
      await selectSession(response.binding.session_id);
    } catch (err) {
      botError = err instanceof Error ? err.message : String(err);
    } finally {
      botActionId = null;
    }
  }

  function openCreateBot() { agentCreation.createBot(); }

  function openEditBot(bot: BotProfile) {
    editingBot = bot;
    botName = bot.display_name;
    botRole = bot.role_description ?? "";
    botAvatar = bot.avatar_ref?.trim() || DEFAULT_BOT_AVATAR;
    botSpecialistId = bot.primary_manuscript_id;
    botExternalAgent = bot.external_agent ? { ...bot.external_agent } : null;
    botWorldBinding = bot.world_binding ? { ...bot.world_binding } : null;
    botError = null;
    botEditorOpen = true;
  }

  function closeBotEditor() {
    if (botSaving) return;
    botEditorOpen = false;
    editingBot = null;
    botWorldBinding = null;
    botError = null;
  }

  async function submitBot(event: SubmitEvent) {
    event.preventDefault();
    if (botSaving || !botName.trim() || !botRole.trim() || !botSpecialistId) return;
    botSaving = true;
    botError = null;
    try {
      if (editingBot) {
        await bots.update(editingBot, {
          display_name: botName.trim(),
          role_description: botRole.trim() || null,
          avatar_ref: botAvatar,
          primary_manuscript_id: botSpecialistId,
          additional_manuscript_ids: editingBot.additional_manuscript_ids ?? [],
          default_mode: editingBot.default_mode ?? null,
          ...botExecutorUpdate(editingBot.external_agent, botExternalAgent),
          world_binding: botWorldBinding ?? undefined,
          clear_world_binding: Boolean(editingBot.world_binding && !botWorldBinding),
        });
        await chat.refreshSessions({ force: true });
        botEditorOpen = false;
        editingBot = null;
      } else {
        const response = await bots.create({
          display_name: botName.trim(),
          role_description: botRole.trim() || null,
          avatar_ref: botAvatar,
          primary_manuscript_id: botSpecialistId,
          additional_manuscript_ids: [],
          default_mode: null,
          external_agent: botExternalAgent,
          world_binding: botWorldBinding ?? undefined,
        });
        await chat.refreshSessions({ force: true });
        botEditorOpen = false;
        await selectSession(response.binding.session_id);
      }
    } catch (err) {
      botError = err instanceof Error ? err.message : String(err);
    } finally {
      botSaving = false;
    }
  }

  async function duplicateBot(bot: BotProfile) {
    if (botActionId) return;
    botActionId = bot.bot_id;
    botError = null;
    try {
      const response = await bots.duplicate(bot);
      await chat.refreshSessions({ force: true });
      await selectSession(response.binding.session_id);
    } catch (err) {
      botError = err instanceof Error ? err.message : String(err);
    } finally {
      botActionId = null;
    }
  }

  async function toggleBotArchived(bot: BotProfile) {
    if (botActionId) return;
    botActionId = bot.bot_id;
    botError = null;
    try {
      await bots.setArchived(bot, !bot.archived);
    } catch (err) {
      botError = err instanceof Error ? err.message : String(err);
    } finally {
      botActionId = null;
    }
  }

  async function createSharedRoom() {
    try {
      await chat.newSharedRoom();
      onPick?.();
      if (variant === "drawer" || variant === "sheet") {
        layout.setSessionDrawerOpen(false);
        onClose?.();
      }
    } catch (err) {
      chat.sessionsError = err instanceof Error ? err.message : String(err);
    }
  }

  function openRename(session: SessionSummary) {
    renamingSession = session;
    renameDraft = session.display_name?.trim() || formatSessionLabel(session);
    renameError = null;
  }

  function closeRename() {
    renamingSession = null;
    renameDraft = "";
    renameError = null;
    renameSaving = false;
  }

  async function submitRename(event: Event) {
    event.preventDefault();
    if (!renamingSession || !renameDraft.trim() || renameSaving) return;
    renameSaving = true;
    renameError = null;
    try {
      const id = renamingSession.session_id;
      const scope = searchScope;
      await chat.renameSession(id, renameDraft.trim());
      if (scope !== searchScope) return;
      // Invalidate an older in-flight search snapshot after the mutation.
      void sessionSearch.search(scope, query);
      closeRename();
    } catch (err) {
      renameError = err instanceof Error ? err.message : String(err);
      renameSaving = false;
    }
  }

  function openDelete(session: SessionSummary) {
    deletingSession = session;
    deleteError = null;
  }

  function closeDelete() {
    deletingSession = null;
    deleteError = null;
    deleteSaving = false;
  }

  async function confirmDelete() {
    if (!deletingSession || deleteSaving) return;
    deleteSaving = true;
    deleteError = null;
    try {
      const id = deletingSession.session_id;
      const scope = searchScope;
      await chat.deleteSession(id);
      if (scope !== searchScope) return;
      void sessionSearch.search(scope, query);
      closeDelete();
      if (variant === "drawer" || variant === "sheet") {
        layout.setSessionDrawerOpen(false);
        onClose?.();
      }
    } catch (err) {
      deleteError = err instanceof Error ? err.message : String(err);
      deleteSaving = false;
    }
  }
</script>

{#if open}
  {#if variant === "sheet"}
    <div
      class="mobile-sheet-backdrop mobile-sheet-peek-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) dismissSheet();
      }}
    >
      <div
        bind:this={sheetEl}
        class="mobile-sheet mobile-sheet-peek relative flex flex-col"
        role="dialog"
        aria-label="Chat sessions"
      >
        <header
          bind:this={headerEl}
          class="mobile-sheet-stack-header mobile-chat-history-header"
        >
          <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
          <div class="mobile-sheet-header-row">
            <h2 class="text-base font-semibold text-surface-50">Sessions</h2>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={dismissSheet}
            >
              Done
            </button>
          </div>
        </header>
        {@render sessionPanelBody()}
      </div>
    </div>
  {:else}
    {#if variant === "drawer"}
      <button
        type="button"
        class="absolute inset-0 z-20 bg-surface-950/50"
        aria-label="Close sessions"
        onclick={onClose}
      ></button>
    {/if}

    <aside
      class="{variant === 'drawer'
        ? 'workshop-drawer absolute left-0 top-0 z-30 w-64 border-r-2'
        : variant === 'inline'
          ? 'relative flex h-full min-h-0 w-full flex-col'
          : 'workshop-drawer relative w-56 shrink-0 border-r-2'} relative flex h-full flex-col"
      aria-label="Chat sessions"
    >
      {#if variant !== "inline"}
        <div class="session-sidebar-header">
          <p class="text-sm font-semibold text-surface-100">Sessions</p>
          {#if onClose}
            <button
              type="button"
              class="session-sidebar-icon-btn"
              aria-label="Close sessions"
              onclick={onClose}
            >
              <X size={15} strokeWidth={1.75} />
            </button>
          {/if}
        </div>
      {/if}
      {@render sessionPanelBody()}
    </aside>
  {/if}
{/if}

{#snippet plainRow(session: SessionSummary, isPinned: boolean, relatedCount = 0)}
  <SessionRow {session} {now} selected={chat.focusedSessionId === session.session_id} pinned={isPinned}
    {relatedCount} relatedExpanded={relatedCount > 0 && familyExpanded(session.session_id)}
    onToggleRelated={relatedCount > 0 ? () => toggleFamily(session.session_id) : undefined}
    alwaysShowActions={touchActions} onSelect={() => void selectSession(session.session_id, formatSessionLabel(session))}
    onRename={() => openRename(session)} onDelete={() => openDelete(session)} onTogglePin={() => toggleSessionPin(session)} />
{/snippet}

{#snippet familyRow(session: SessionSummary, isPinned: boolean)}
  {@const children = families.children.get(session.session_id) ?? []}
  {@render plainRow(session, isPinned, query.trim() ? 0 : children.length)}
  {#if query.trim() && families.parents.has(session.session_id)}
    {@const parent = candidates.find(item => item.session_id === families.parents.get(session.session_id))}
    {#if parent}<button type="button" class="workshop-faint ml-6 truncate text-[10px]" onclick={() => void selectSession(parent.session_id)}>From: {formatSessionLabel(parent)}</button>{/if}
  {:else if !query.trim() && children.length && familyExpanded(session.session_id)}
    <ul class="session-sidebar-section-list session-family-children" aria-label={'Related chats for ' + formatSessionLabel(session)}>
      {#each children as child (child.session_id)}<li>{@render plainRow(child, false)}</li>{/each}
    </ul>
  {/if}
{/snippet}

{#snippet sessionPanelBody()}
  <div class="flex min-h-0 flex-1 flex-col">
    {#if showBuiltInToolbar}
      {#if variant === "inline"}
        <header class="lme-side-rail-dock">
          <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
            <Search
              size={14}
              strokeWidth={1.75}
              class="shrink-0 text-content-quiet"
              aria-hidden="true"
            />
            <input
              bind:this={searchInputEl}
              class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
              type="search"
              placeholder="Search all chats…"
              bind:value={query}
            />
          </div>
          <button
            type="button"
            class="vault-dock-icon-btn"
            title="New chat"
            aria-label="New chat"
            onclick={createSession}
          >
            <Plus size={16} strokeWidth={1.75} />
          </button>
          {#if sharedMode.isShared}
            <button
              type="button"
              class="vault-dock-icon-btn"
              title="New shared room"
              aria-label="New shared room"
              onclick={() => void createSharedRoom()}
            >
              <Users size={15} strokeWidth={1.75} />
            </button>
          {/if}
        </header>
      {:else}
        <div class="session-sidebar-toolbar {variant === 'sheet' ? 'session-sidebar-toolbar--sheet' : ''}">
          <label class="session-sidebar-search">
            <Search size={14} strokeWidth={1.75} class="session-sidebar-search-icon" aria-hidden="true" />
            <input
              bind:this={searchInputEl}
              class="session-sidebar-search-input"
              type="search"
              placeholder="Search titles…"
              bind:value={query}
            />
          </label>
          <button
            type="button"
            class="session-sidebar-new"
            title="New chat"
            aria-label="New chat"
            onclick={createSession}
          >
            <Plus size={15} strokeWidth={2} />
            <span class="session-sidebar-new-label">New</span>
          </button>
          {#if sharedMode.isShared}
            <button
              type="button"
              class="session-sidebar-new"
              title="New shared room"
              aria-label="New shared room"
              onclick={() => void createSharedRoom()}
            >
              <Users size={15} strokeWidth={2} />
              <span class="session-sidebar-new-label">Room</span>
            </button>
          {/if}
        </div>
      {/if}
    {/if}

    {#if chat.sessionsError}
      <p class="px-3 py-2 text-xs text-content-error">{chat.sessionsError}</p>
    {/if}
    {#if botError}
      <div class="flex items-start gap-2 px-3 py-2 text-xs text-content-error" role="alert">
        <span class="min-w-0 flex-1">{botError}</span>
        <button type="button" class="shrink-0 text-content-tertiary" onclick={() => (botError = null)}>
          Dismiss
        </button>
      </div>
    {/if}

    <ol class="session-sidebar-list {variant === 'sheet' ? 'mobile-chat-history-list' : ''}">
      <li class="session-sidebar-section">
        <SidebarSectionHeading id="bots" label="Bots" count={activeBots.length} searching={Boolean(query.trim())}>
          {#snippet children()}
            <button
              type="button"
              class="session-sidebar-heading-action"
              title="New Bot"
              aria-label="New Bot"
              onclick={() => void openCreateBot()}
            >
              <Plus size={13} strokeWidth={2} />
            </button>
          {/snippet}
          {#snippet content()}
        {#if activeBots.length > 0}
          <ul class="session-sidebar-section-list">
            {#each activeBots as bot (bot.bot_id)}
              <li class:opacity-60={botActionId === bot.bot_id}>
                <BotRow
                  {bot}
                  specialistLabel={specialistLabel(bot)}
                  selected={bots.forSession(chat.focusedSessionId)?.bot_id === bot.bot_id}
                  alwaysShowActions={touchActions}
                  onSelect={() => void selectBot(bot)}
                  onEdit={() => openEditBot(bot)}
                  onDuplicate={() => void duplicateBot(bot)}
                  onArchive={() => void toggleBotArchived(bot)}
                />
              </li>
            {/each}
          </ul>
        {:else if bots.loading}
          <p class="workshop-faint px-4 py-2 text-[11px]">Loading Bots…</p>
        {:else if !query.trim()}
          <div class="session-row">
            <button
              type="button"
              class="session-row-main"
              onclick={() => void openCreateBot()}
            >
              <span class="session-row-title">Create a Bot</span>
            </button>
          </div>
        {/if}
          {/snippet}
        </SidebarSectionHeading>
      </li>

      <ConnectedAgentList {open} {query} onSelect={(sessionId, title) => void selectSession(sessionId, title)} />

      <li class="session-sidebar-section">
        <SidebarSectionHeading id="pinned" label="Pinned" count={pinned.length} searching={Boolean(query.trim())}>
          {#snippet content()}
            {#if pinned.length > 0}
              <ul class="session-sidebar-section-list">
                {#each pinned as session (session.session_id)}
                  <li>{@render familyRow(session, true)}</li>
                {/each}
              </ul>
            {/if}
          {/snippet}
        </SidebarSectionHeading>
      </li>

      {#if archivedBots.length > 0}
        <li class="session-sidebar-section">
          <button
            type="button"
            class="session-sidebar-section-heading session-sidebar-section-heading--button"
            aria-expanded={!archivedBotsCollapsed}
            onclick={() => (archivedBotsCollapsed = !archivedBotsCollapsed)}
          >
            <span class="session-sidebar-section-title">Archived Bots</span>
            <span class="session-sidebar-section-heading__trailing">
              <span class="session-sidebar-section-count">{archivedBots.length}</span>
              {#if archivedBotsCollapsed}
                <ChevronRight size={12} strokeWidth={1.75} aria-hidden="true" />
              {:else}
                <ChevronDown size={12} strokeWidth={1.75} aria-hidden="true" />
              {/if}
            </span>
          </button>
          {#if !archivedBotsCollapsed}
            <ul class="session-sidebar-section-list">
              {#each archivedBots as bot (bot.bot_id)}
                <li class:opacity-60={botActionId === bot.bot_id}>
                  <BotRow
                    {bot}
                    specialistLabel={specialistLabel(bot)}
                    alwaysShowActions={touchActions}
                    onSelect={() => openEditBot(bot)}
                    onEdit={() => openEditBot(bot)}
                    onDuplicate={() => void duplicateBot(bot)}
                    onArchive={() => void toggleBotArchived(bot)}
                  />
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/if}

      {#if recent.length > 0}
        {#each recentGroups as group (group.id)}
          <li class="session-sidebar-section">
            <SidebarSectionHeading id={group.id} label={group.label} count={group.sessions.length} searching={Boolean(query.trim())} initiallyCollapsed={group.id === 'older'}>
              {#snippet content()}
              <ul class="session-sidebar-section-list">
                {#each group.sessions as session (session.session_id)}
                  <li>{@render familyRow(session, false)}</li>
                {/each}
              </ul>
              {/snippet}
            </SidebarSectionHeading>
          </li>
        {/each}
      {:else if listEmpty && !sessionSearch.loading && !sessionSearch.error && bots.bots.length === 0}
        <li class="session-sidebar-empty">
          {#if query.trim()}
            No sessions match “{query.trim()}”.
          {:else}
            No chats for {userProfiles.activeDisplayName} yet.
            <span class="mt-2 block workshop-faint">
              Work and home stay separate — switch profile anytime in Settings → Memory.
            </span>
          {/if}
        </li>
      {/if}
      {#if query.trim() && sessionSearch.loading}
        <li class="workshop-faint px-3 py-2 text-xs" role="status">Searching all chats…</li>
      {/if}
      {#if query.trim() && sessionSearch.error}
        <li class="px-3 py-2 text-xs text-content-error" role="alert">
          Search is incomplete: {sessionSearch.error}
          <button type="button" class="session-sidebar-new" onclick={() => void sessionSearch.search(searchScope, query)}>Retry search</button>
        </li>
      {/if}
      {#if !query.trim() && chat.sessionsCursor}
        <li class="px-3 py-2">
          <button type="button" class="session-sidebar-new" disabled={chat.sessionsLoadingOlder || chat.sessionsRefreshing} onclick={() => void loadMoreSessions(chat)}>
            {chat.sessionsLoadingOlder ? "Loading older chats…" : "Load older chats"}
          </button>
        </li>
      {/if}
      {#if chat.sessionsError}
        <li class="px-3 py-2"><button type="button" class="session-sidebar-new" disabled={chat.sessionsRefreshing} onclick={() => void chat.refreshSessions({ force: true })}>Retry loading chats</button></li>
      {/if}
    </ol>
  </div>

  {#if renamingSession}
    <div
      class="absolute inset-0 z-40 flex items-end bg-surface-950/70 p-3 sm:items-center sm:justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="session-rename-title"
    >
      <form
        class="card w-full space-y-3 p-4 shadow-xl sm:max-w-sm"
        onsubmit={submitRename}
      >
        <div class="flex items-start justify-between gap-3">
          <div>
            <p id="session-rename-title" class="text-sm font-semibold text-surface-100">
              Rename session
            </p>
            <p class="workshop-faint mt-0.5 text-xs">
              Saved on this device — searchable in your session list.
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            aria-label="Cancel rename"
            onclick={closeRename}
          >
            <X size={16} strokeWidth={1.75} />
          </button>
        </div>
        <input
          bind:this={renameInputEl}
          class="input w-full text-sm"
          type="text"
          maxlength="80"
          bind:value={renameDraft}
        />
        {#if renameError}
          <p class="text-xs text-content-error">{renameError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={closeRename}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="btn btn-sm variant-filled-primary"
            disabled={!renameDraft.trim() || renameSaving}
          >
            {renameSaving ? "Saving…" : "Save name"}
          </button>
        </div>
      </form>
    </div>
  {/if}

  {#if deletingSession}
    <div
      class="absolute inset-0 z-40 flex items-end bg-surface-950/70 p-3 sm:items-center sm:justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="session-delete-title"
    >
      <div class="card w-full space-y-3 p-4 shadow-xl sm:max-w-sm">
        <div class="flex items-start justify-between gap-3">
          <div>
            <p id="session-delete-title" class="text-sm font-semibold text-surface-100">
              Delete session?
            </p>
            <p class="workshop-faint mt-0.5 text-xs">
              Removes transcript, catalog entry, and Locus memory for
              {formatSessionLabel(deletingSession)}. This cannot be undone.
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            aria-label="Cancel delete"
            onclick={closeDelete}
          >
            <X size={16} strokeWidth={1.75} />
          </button>
        </div>
        {#if deleteError}
          <p class="text-xs text-content-error">{deleteError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={closeDelete}
          >
            Cancel
          </button>
          <button
            type="button"
            class="btn btn-sm variant-filled-error"
            disabled={deleteSaving}
            onclick={confirmDelete}
          >
            {deleteSaving ? "Deleting…" : "Delete"}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if botEditorOpen}
    <BotEditor bind:name={botName} bind:purpose={botRole} bind:avatar={botAvatar}
      bind:archetypeId={botSpecialistId} bind:worldBinding={botWorldBinding} bind:externalAgent={botExternalAgent}
      editing={Boolean(editingBot)} saving={botSaving} error={botError}
      onclose={closeBotEditor} onsubmit={submitBot} />
  {/if}
{/snippet}
