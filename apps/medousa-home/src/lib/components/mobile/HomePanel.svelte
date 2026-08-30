<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import "$lib/styles/mobile-home-convergence.postcss";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import WorkAsksPanel from "$lib/components/work/WorkAsksPanel.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { calendar } from "$lib/stores/calendar.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { listCalendarEvents, retryWorkspaceCard } from "$lib/daemon";
  import { buildMotionSummary, motionColumnCounts } from "$lib/utils/mobilePulse";
  import { dailyNotePath } from "$lib/utils/vaultTemplates";
  import { partitionWorkHub } from "$lib/utils/workHub";
  import {
    fetchPeerHomePreview,
    type PeerHomePreview,
  } from "$lib/utils/peerHomePreview";
  import {
    homeContinueRows,
    homeNotesDateParts,
    homeProjectRows,
    type HomeProjectRow,
  } from "$lib/utils/homeContinue";
  import { homeTodayAgenda } from "$lib/utils/homeToday";
  import type { CalendarEvent } from "$lib/types/calendar";
  import {
    listForgeRepositories,
    type RepositoryCatalogEntry,
  } from "$lib/forge";
  import { isTauri } from "$lib/window";
  import { haptic } from "$lib/haptics";
  import { enterMobileCodeProject } from "$lib/utils/mobileCodeOpen";
  import { Radio, Users } from "@lucide/svelte";

  interface Props {
    health: DaemonHealth | null;
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenChat: (sessionId?: string) => void | Promise<void>;
    onOpenNote: (path: string) => void | Promise<void>;
    onOpenSettings: () => void;
  }

  let {
    health,
    onSelectCard,
    onOpenChat,
    onOpenNote,
    onOpenSettings,
  }: Props = $props();

  const blocked = $derived(workspace.needsAttentionCount());
  const inMotion = $derived(workspace.inMotionCount());
  let now = $state(new Date());
  const todayDailyPath = $derived(dailyNotePath(now));
  const partition = $derived(partitionWorkHub(workspace.cards));
  const living = $derived(partition.living);

  let peerPreview = $state<PeerHomePreview>({
    unreadTotal: 0,
    peerCount: 0,
    stripThreads: [],
    latestThread: null,
  });
  let peerPollTimer: ReturnType<typeof setInterval> | undefined;
  let clockTimer: ReturnType<typeof setInterval> | undefined;
  let todayCalendarEvents = $state<CalendarEvent[]>([]);
  let sessionsHydrated = $state(false);
  let calendarHydrated = $state(false);

  const isOffline = $derived(health !== null && !health.ok);
  const isConnecting = $derived(health === null);

  const motionSummary = $derived(
    buildMotionSummary(motionColumnCounts(workspace.cards)),
  );

  const greeting = $derived.by(() => {
    const hour = now.getHours();
    if (hour < 5) return "Late night";
    if (hour < 12) return "Good morning";
    if (hour < 18) return "Good afternoon";
    return "Good evening";
  });

  const statusLine = $derived.by(() => {
    if (isOffline) return "Not connected";
    if (isConnecting) return "Connecting…";
    if (blocked > 0) {
      return blocked === 1 ? "1 thing needs you" : `${blocked} things need you`;
    }
    if (living.length > 0) {
      return motionSummary ?? `${inMotion} in motion`;
    }
    return null;
  });

  const statusTone = $derived.by((): "alive" | "warn" | "idle" => {
    if (!health) return "idle";
    if (!health.ok || blocked > 0) return "warn";
    return "alive";
  });

  const notesDate = $derived(homeNotesDateParts(now));
  const notesWhisper = $derived(
    vault.notes.some((note) => note.path === todayDailyPath)
      ? "Open today’s note"
      : "Start today’s note",
  );
  const todayAgenda = $derived(
    homeTodayAgenda(todayCalendarEvents, now, 3),
  );

  const continueRows = $derived(homeContinueRows(chat.sessions, 3));
  const continueLead = $derived(continueRows[0] ?? null);
  const continueWhispers = $derived(continueRows.slice(1));

  let projectCatalog = $state<RepositoryCatalogEntry[]>([]);
  const projectRows = $derived(homeProjectRows(projectCatalog, 3));
  const projectLead = $derived(projectRows[0] ?? null);
  const projectWhispers = $derived(projectRows.slice(1));
  let projectsHydrated = $state(false);

  const firstBlockedCardId = $derived.by(() => {
    const blockedCard =
      workspace.cards.find((card) => card.column === "blocked") ??
      partition.stuck[0] ??
      null;
    return blockedCard?.id ?? null;
  });

  const peersHaveSignal = $derived(peerPreview.unreadTotal > 0);
  const peerSignalLabel = $derived(
    peerPreview.unreadTotal === 1
      ? "1 new message"
      : `${peerPreview.unreadTotal} new messages`,
  );

  let scrollEl: HTMLDivElement | undefined = $state();
  let pullY = $state(0);
  let refreshing = $state(false);
  let touchStartY = 0;
  let pulling = false;

  let toastMessage = $state<string | null>(null);
  let toastCardId = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (sessionsHydrated) return;
    if (!health?.ok) return;
    sessionsHydrated = true;
    void chat.refreshSessions();
  });

  $effect(() => {
    if (projectsHydrated) return;
    if (!health?.ok) return;
    projectsHydrated = true;
    void refreshProjects();
  });

  $effect(() => {
    if (calendarHydrated) return;
    if (!health?.ok) return;
    calendarHydrated = true;
    void refreshTodayCalendar();
  });

  $effect(() => {
    // Optional work detail — never contend with first paint.
    const run = () => void workspace.prefetchCardDetails();
    let idleId: number | undefined;
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    if (typeof requestIdleCallback === "function") {
      idleId = requestIdleCallback(run, { timeout: 4000 });
    } else {
      timeoutId = setTimeout(run, 2000);
    }
    return () => {
      if (idleId !== undefined && typeof cancelIdleCallback === "function") {
        cancelIdleCallback(idleId);
      }
      if (timeoutId) clearTimeout(timeoutId);
    };
  });

  onMount(() => {
    clockTimer = setInterval(() => {
      const next = new Date();
      const crossedDay = dailyNotePath(next) !== dailyNotePath(now);
      now = next;
      if (crossedDay) void refreshTodayCalendar();
    }, 60_000);
    if (!isTauri()) {
      void chat.refreshSessions();
      void refreshProjects();
      void refreshTodayCalendar();
      return;
    }
    void refreshPeerPreview();
    peerPollTimer = setInterval(() => {
      if (document.visibilityState !== "visible") return;
      void refreshPeerPreview();
    }, 30_000);
  });

  onDestroy(() => {
    if (peerPollTimer) clearInterval(peerPollTimer);
    if (clockTimer) clearInterval(clockTimer);
  });

  async function refreshPeerPreview() {
    peerPreview = await fetchPeerHomePreview();
  }

  async function refreshProjects() {
    try {
      projectCatalog = await listForgeRepositories();
    } catch {
      // Forge may be unavailable — Home just omits the projects block.
      projectCatalog = [];
    }
  }

  async function refreshTodayCalendar() {
    const dayStart = new Date(now);
    dayStart.setHours(0, 0, 0, 0);
    const from = new Date(dayStart);
    from.setDate(from.getDate() - 1);
    const to = new Date(dayStart);
    to.setDate(to.getDate() + 2);

    try {
      const response = await listCalendarEvents({
        from: from.toISOString(),
        to: to.toISOString(),
        path: calendar.calendarPath,
      });
      todayCalendarEvents = response.events;
    } catch {
      // Calendar is optional on Home — no error tile and no empty-state copy.
      todayCalendarEvents = [];
    }
  }

  function onStatusTap() {
    if (isOffline) {
      onOpenSettings();
      return;
    }
    if (blocked > 0 && firstBlockedCardId) {
      haptic("light");
      void onSelectCard(firstBlockedCardId);
      return;
    }
    if (peerPreview.unreadTotal > 0) {
      openPeers();
    }
  }

  async function openDailyNote() {
    haptic("light");
    const path = todayDailyPath;
    const exists = vault.notes.some((note) => note.path === path);
    if (!exists) {
      await vault.createDailyNote();
    }
    await onOpenNote(path);
  }

  function openPeers() {
    haptic("light");
    layout.openMore("peers");
  }

  function openCalendar(event?: CalendarEvent) {
    haptic("light");
    calendar.selectDay(now);
    if (event) calendar.openEdit(event);
    layout.openMore("calendar");
  }

  async function openContinue(sessionId: string) {
    haptic("light");
    await onOpenChat(sessionId);
  }

  async function openProject(row: HomeProjectRow) {
    haptic("light");
    layout.openMore("code");
    if (row.workId) {
      await enterMobileCodeProject(row.workId);
    }
  }

  async function refresh() {
    await Promise.all([
      workspace.prefetchCardDetails(),
      refreshPeerPreview(),
      chat.refreshSessions(),
      refreshProjects(),
      refreshTodayCalendar(),
      vault.notes.length === 0 ? vault.refreshNotes() : Promise.resolve(),
    ]);
  }

  function onTouchStart(event: TouchEvent) {
    if (!scrollEl || scrollEl.scrollTop > 2 || refreshing) return;
    touchStartY = event.touches[0].clientY;
    pulling = true;
  }

  function onTouchMove(event: TouchEvent) {
    if (!pulling || !scrollEl || scrollEl.scrollTop > 2) return;
    const delta = event.touches[0].clientY - touchStartY;
    if (delta > 0) {
      pullY = Math.min(delta * 0.45, 72);
    }
  }

  async function onTouchEnd() {
    if (!pulling) return;
    pulling = false;
    if (pullY >= 48) {
      refreshing = true;
      try {
        await refresh();
        haptic("success");
      } finally {
        refreshing = false;
      }
    }
    pullY = 0;
  }

  function dismissToast() {
    if (toastTimer) clearTimeout(toastTimer);
    toastMessage = null;
    toastCardId = null;
  }

  async function undoCancel() {
    if (!toastCardId) return;
    const cardId = toastCardId;
    dismissToast();
    haptic("light");
    try {
      await retryWorkspaceCard(cardId);
    } catch (err) {
      toastMessage = err instanceof Error ? err.message : String(err);
      toastCardId = null;
      toastTimer = setTimeout(dismissToast, 4000);
    }
  }
</script>

{#if workspace.workView === "asks"}
  <WorkAsksPanel {onOpenChat} />
{:else}
<section class="mobile-home relative flex h-full min-h-0 flex-col">
  <div
    bind:this={scrollEl}
    class="mobile-pull-scroll min-h-0 flex-1 overflow-y-auto"
    role="region"
    aria-label="Home"
    ontouchstart={onTouchStart}
    ontouchmove={onTouchMove}
    ontouchend={onTouchEnd}
  >
    {#if pullY > 0 || refreshing}
      <div
        class="mobile-pull-indicator"
        style:height="{pullY || (refreshing ? 32 : 0)}px"
      >
        <span class="workshop-faint text-xs">
          {refreshing ? "Refreshing…" : pullY >= 48 ? "Release to refresh" : "Pull to refresh"}
        </span>
      </div>
    {/if}

    <div class="px-5 pb-8 pt-3">
      <section
        class="mobile-home-today mobile-home-rise"
        style="--home-rise-delay: 0ms"
        aria-labelledby="mobile-home-today-heading"
      >
        <h1
          id="mobile-home-today-heading"
          class="mobile-home-today-kicker"
        >{greeting}</h1>
        <button
          type="button"
          class="mobile-home-today-note"
          onclick={() => void openDailyNote()}
        >
          <span class="mobile-home-today-date">
            <span class="mobile-home-today-weekday">{notesDate.weekday}</span>
            <span class="mobile-home-today-day">{notesDate.day}</span>
          </span>
          <span class="mobile-home-today-note-label">{notesWhisper}</span>
        </button>

        {#if todayAgenda.rows.length > 0}
          <div class="mobile-home-today-events" aria-label="Remaining events today">
            {#each todayAgenda.rows as row (`${row.event.uid}:${row.event.recurrence_id ?? row.event.dtstart}`)}
              <button
                type="button"
                class="mobile-home-today-event"
                onclick={() => openCalendar(row.event)}
              >
                <span
                  class="mobile-home-today-event-time"
                  class:mobile-home-today-event-time--now={row.timing === "now"}
                >{row.timeLabel}</span>
                <span class="mobile-home-today-event-title">{row.title}</span>
              </button>
            {/each}
            {#if todayAgenda.hiddenCount > 0}
              <button
                type="button"
                class="mobile-home-today-more"
                onclick={() => openCalendar()}
              >See {todayAgenda.hiddenCount} more</button>
            {/if}
          </div>
        {/if}
      </section>

      {#if statusLine || peersHaveSignal}
        <div
          class="mobile-home-meta mobile-home-signals mobile-home-rise"
          style="--home-rise-delay: 40ms"
          aria-label="Workspace activity"
        >
          {#if statusLine}
            <button
              type="button"
              class="mobile-home-meta-row"
              onclick={onStatusTap}
            >
              <span class="mobile-home-meta-lead" aria-hidden="true">
                <span
                  class="mobile-home-meta-mark mobile-home-meta-mark--status"
                  class:mobile-home-meta-mark--alive={statusTone === "alive"}
                  class:mobile-home-meta-mark--warn={statusTone === "warn"}
                  class:mobile-home-meta-mark--idle={statusTone === "idle"}
                >
                  <Radio size={13} strokeWidth={1.75} />
                </span>
              </span>
              <span class="mobile-home-meta-label">{statusLine}</span>
            </button>
          {/if}
          {#if peersHaveSignal}
            <button
              type="button"
              class="mobile-home-meta-row"
              onclick={openPeers}
            >
              <span class="mobile-home-meta-lead" aria-hidden="true">
                <span class="mobile-home-meta-mark">
                  <Users size={13} strokeWidth={1.75} />
                </span>
              </span>
              <span class="mobile-home-meta-label">{peerSignalLabel}</span>
            </button>
          {/if}
        </div>
      {/if}

      {#if continueLead}
        <div class="mobile-home-continue mobile-home-rise" style="--home-rise-delay: 60ms">
          <p class="mobile-home-continue-kicker">Continue</p>
          <button
            type="button"
            class="mobile-home-continue-lead"
            onclick={() => void openContinue(continueLead.sessionId)}
          >
            <div class="mobile-home-continue-lead-top">
              <span class="mobile-home-continue-lead-title">{continueLead.title}</span>
              {#if continueLead.relativeTime}
                <span class="mobile-home-continue-time">{continueLead.relativeTime}</span>
              {/if}
            </div>
            {#if continueLead.preview}
              <span class="mobile-home-continue-preview">{continueLead.preview}</span>
            {/if}
          </button>
          {#each continueWhispers as row (row.sessionId)}
            <button
              type="button"
              class="mobile-home-continue-whisper"
              onclick={() => void openContinue(row.sessionId)}
            >
              <span class="mobile-home-continue-whisper-title">{row.title}</span>
              {#if row.relativeTime}
                <span class="mobile-home-continue-time">{row.relativeTime}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if projectLead}
        <div class="mobile-home-continue mobile-home-rise" style="--home-rise-delay: 110ms">
          <p class="mobile-home-continue-kicker">Projects</p>
          <button
            type="button"
            class="mobile-home-continue-lead"
            onclick={() => void openProject(projectLead)}
          >
            <div class="mobile-home-continue-lead-top">
              <span class="mobile-home-continue-lead-title">{projectLead.title}</span>
              {#if projectLead.relativeTime}
                <span class="mobile-home-continue-time">{projectLead.relativeTime}</span>
              {/if}
            </div>
            {#if projectLead.preview}
              <span class="mobile-home-continue-preview">{projectLead.preview}</span>
            {/if}
          </button>
          {#each projectWhispers as row (row.path)}
            <button
              type="button"
              class="mobile-home-continue-whisper"
              onclick={() => void openProject(row)}
            >
              <span class="mobile-home-continue-whisper-title">{row.title}</span>
              {#if row.relativeTime}
                <span class="mobile-home-continue-time">{row.relativeTime}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <MobileToast
    message={toastMessage}
    actionLabel="Undo"
    onAction={undoCancel}
    onDismiss={dismissToast}
  />
</section>
{/if}
