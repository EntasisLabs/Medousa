<script lang="ts">
  import "$lib/styles/chat.postcss";
  import { onMount, untrack } from "svelte";
  import { ChevronDown, ChevronRight, Plus, Search, Sparkles, Users, X } from "@lucide/svelte";
  import BotRow from "$lib/components/chat/BotRow.svelte";
  import SessionRow from "$lib/components/chat/SessionRow.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { chat } from "$lib/stores/chat.svelte";
  import { activeAgent } from "$lib/stores/activeAgent.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { sharedMode } from "$lib/stores/sharedMode.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { SessionSummary } from "$lib/types/session";
  import type { BotProfile } from "$lib/types/generated/daemon_api";
  import { formatSessionLabel } from "$lib/utils/formatSession";
  import { groupSessionsByRecency } from "$lib/utils/sessionHistoryGroups";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    onClose?: () => void;
    /** Fired after a session is opened/created (e.g. close a rail popover). */
    onPick?: () => void;
    variant?: "drawer" | "inline" | "sheet";
    /** `rail-list` hides the built-in toolbar (actions live in the rail popover strip). */
    chrome?: "default" | "rail-list";
  }

  let { open, onClose, onPick, variant = "drawer", chrome = "default" }: Props = $props();

  const showBuiltInToolbar = $derived(chrome !== "rail-list");

  let query = $state("");
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
  let olderCollapsed = $state(true);
  let archivedBotsCollapsed = $state(true);
  let botEditorOpen = $state(false);
  let editingBot = $state<BotProfile | null>(null);
  let botName = $state("");
  let botRole = $state("");
  let botAvatar = $state("✨");
  let botSpecialistId = $state("");
  let botSaving = $state(false);
  let botError = $state<string | null>(null);
  let botActionId = $state<string | null>(null);

  const BOT_AVATARS = ["✨", "🧭", "🧠", "🛠️", "📚", "🔭", "🎨", "🌱"];

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

  const pinned = $derived(
    chat.sessions.filter(
      (session) =>
        !botSessionIds.has(session.session_id) &&
        chat.isPinned(session.session_id) &&
        matchesQuery(session),
    ),
  );

  const recent = $derived(
    chat.sessions.filter(
      (session) =>
        !botSessionIds.has(session.session_id) &&
        !chat.isPinned(session.session_id) &&
        matchesQuery(session),
    ),
  );
  const recentGroups = $derived(groupSessionsByRecency(recent));

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

  async function selectSession(sessionId: string) {
    // Shell tabs own visible chat selection. Activating through the shell also
    // switches/hydrates the chat store, without relying on its async mirror.
    const tabId = shellTabs.openChat(sessionId, { activate: true });
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

  async function openCreateBot() {
    if (catalog.manuscripts.length === 0 && !catalog.loading) {
      await catalog.refresh();
    }
    editingBot = null;
    botName = "";
    botRole = "";
    botAvatar = "✨";
    botSpecialistId =
      activeAgent.selectedManuscriptId ?? catalog.manuscripts[0]?.id ?? "";
    botError = null;
    botEditorOpen = true;
  }

  function openEditBot(bot: BotProfile) {
    editingBot = bot;
    botName = bot.display_name;
    botRole = bot.role_description ?? "";
    botAvatar = bot.avatar_ref?.trim() || "✨";
    botSpecialistId = bot.primary_manuscript_id;
    botError = null;
    botEditorOpen = true;
  }

  function closeBotEditor() {
    if (botSaving) return;
    botEditorOpen = false;
    editingBot = null;
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
      await chat.renameSession(renamingSession.session_id, renameDraft.trim());
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
      await chat.deleteSession(deletingSession.session_id);
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
              placeholder="Search titles…"
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
    {:else if chat.sessionsRefreshing}
      <p class="workshop-faint px-3 py-1 text-[11px]">Updating sessions…</p>
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
        <div class="session-sidebar-section-heading">
          <p class="session-sidebar-section-title">Bots</p>
          <span class="session-sidebar-section-heading__trailing">
            {#if activeBots.length > 0}
              <span class="session-sidebar-section-count">{activeBots.length}</span>
            {/if}
            <button
              type="button"
              class="session-sidebar-heading-action"
              title="New Bot"
              aria-label="New Bot"
              onclick={() => void openCreateBot()}
            >
              <Plus size={13} strokeWidth={2} />
            </button>
          </span>
        </div>
        {#if activeBots.length > 0}
          <ul class="session-sidebar-section-list">
            {#each activeBots as bot (bot.bot_id)}
              <li class:opacity-60={botActionId === bot.bot_id}>
                <BotRow
                  {bot}
                  specialistLabel={specialistLabel(bot)}
                  selected={bot.primary_session_id === chat.sessionId}
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
          <button
            type="button"
            class="bot-sidebar-empty-action"
            onclick={() => void openCreateBot()}
          >
            <Sparkles size={13} strokeWidth={1.75} />
            <span>Create a durable teammate</span>
          </button>
        {/if}
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

      {#if pinned.length > 0}
        <li class="session-sidebar-section">
          <div class="session-sidebar-section-heading">
            <p class="session-sidebar-section-title">Pinned</p>
            <span class="session-sidebar-section-count">{pinned.length}</span>
          </div>
          <ul class="session-sidebar-section-list">
            {#each pinned as session (session.session_id)}
              <li>
                <SessionRow
                  {session}
                  selected={chat.sessionId === session.session_id}
                  pinned
                  alwaysShowActions={touchActions}
                  onSelect={() => void selectSession(session.session_id)}
                  onRename={() => openRename(session)}
                  onDelete={() => openDelete(session)}
                  onTogglePin={() => chat.togglePin(session.session_id)}
                />
              </li>
            {/each}
          </ul>
        </li>
      {/if}

      {#if recent.length > 0}
        {#each recentGroups as group (group.id)}
          {@const collapsed = group.id === "older" && olderCollapsed && !query.trim()}
          <li class="session-sidebar-section">
            {#if group.id === "older"}
              <button
                type="button"
                class="session-sidebar-section-heading session-sidebar-section-heading--button"
                aria-expanded={!collapsed}
                onclick={() => (olderCollapsed = !olderCollapsed)}
              >
                <span class="session-sidebar-section-title">{group.label}</span>
                <span class="session-sidebar-section-heading__trailing">
                  <span class="session-sidebar-section-count">{group.sessions.length}</span>
                  {#if collapsed}
                    <ChevronRight size={12} strokeWidth={1.75} aria-hidden="true" />
                  {:else}
                    <ChevronDown size={12} strokeWidth={1.75} aria-hidden="true" />
                  {/if}
                </span>
              </button>
            {:else}
              <div class="session-sidebar-section-heading">
                <p class="session-sidebar-section-title">{group.label}</p>
                <span class="session-sidebar-section-count">{group.sessions.length}</span>
              </div>
            {/if}
            {#if !collapsed}
              <ul class="session-sidebar-section-list">
                {#each group.sessions as session (session.session_id)}
                  <li>
                    <SessionRow
                      {session}
                      selected={chat.sessionId === session.session_id}
                      pinned={false}
                      alwaysShowActions={touchActions}
                      onSelect={() => void selectSession(session.session_id)}
                      onRename={() => openRename(session)}
                      onDelete={() => openDelete(session)}
                      onTogglePin={() => chat.togglePin(session.session_id)}
                    />
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      {:else if listEmpty && bots.bots.length === 0}
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
    <div
      class="absolute inset-0 z-40 flex items-end bg-surface-950/70 p-3 sm:items-center sm:justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="bot-editor-title"
    >
      <form
        class="card max-h-full w-full overflow-y-auto p-4 shadow-xl sm:max-w-md"
        onsubmit={submitBot}
      >
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <p id="bot-editor-title" class="text-sm font-semibold text-surface-100">
              {editingBot ? "Edit Bot" : "New Bot"}
            </p>
            <p class="workshop-faint mt-0.5 text-xs">
              A named teammate with its own memory and conversation.
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm shrink-0 variant-ghost-surface"
            aria-label="Close Bot editor"
            disabled={botSaving}
            onclick={closeBotEditor}
          >
            <X size={16} strokeWidth={1.75} />
          </button>
        </div>

        <div class="mt-4 space-y-4">
          <fieldset>
            <legend class="workshop-label mb-2">Avatar</legend>
            <div class="flex flex-wrap gap-1.5">
              {#each BOT_AVATARS as avatar (avatar)}
                <button
                  type="button"
                  class="bot-avatar-option"
                  class:bot-avatar-option--selected={botAvatar === avatar}
                  aria-label="Use {avatar} avatar"
                  aria-pressed={botAvatar === avatar}
                  onclick={() => (botAvatar = avatar)}
                >
                  {avatar}
                </button>
              {/each}
            </div>
          </fieldset>

          <label class="block">
            <span class="workshop-label">Name</span>
            <input
              class="input mt-1.5 w-full text-sm"
              type="text"
              maxlength="80"
              placeholder="Ada"
              required
              bind:value={botName}
            />
          </label>

          <label class="block">
            <span class="workshop-label">Job</span>
            <textarea
              class="textarea mt-1.5 min-h-20 w-full resize-y text-sm"
              maxlength="500"
              placeholder="Helps me understand systems and connect the concepts."
              required
              bind:value={botRole}
            ></textarea>
          </label>

          <label class="block">
            <span class="workshop-label">Specialist</span>
            <select
              class="select mt-1.5 w-full text-sm"
              required
              bind:value={botSpecialistId}
            >
              <option value="" disabled>Choose a Specialist</option>
              {#each catalog.manuscripts as manuscript (manuscript.id)}
                <option value={manuscript.id}>{manuscript.name}</option>
              {/each}
            </select>
            <span class="workshop-faint mt-1.5 block text-[11px]">
              Expertise stays reusable; this Bot keeps the relationship and memory.
            </span>
          </label>

          {#if catalog.error}
            <p class="text-xs text-content-error">{catalog.error}</p>
          {/if}
          {#if botError}
            <p class="text-xs text-content-error" role="alert">{botError}</p>
          {/if}
        </div>

        <div class="mt-5 flex justify-end gap-2">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={botSaving}
            onclick={closeBotEditor}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="btn btn-sm variant-filled-primary"
            disabled={botSaving || !botName.trim() || !botRole.trim() || !botSpecialistId}
          >
            {botSaving ? "Saving…" : editingBot ? "Save" : "Create Bot"}
          </button>
        </div>
      </form>
    </div>
  {/if}
{/snippet}
