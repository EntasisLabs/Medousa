<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import "$lib/styles/mobile-home-convergence.postcss";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import WorkAsksPanel from "$lib/components/work/WorkAsksPanel.svelte";
  import { Menu } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { retryWorkspaceCard } from "$lib/daemon";
  import { buildMotionSummary, motionColumnCounts } from "$lib/utils/mobilePulse";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { dailyNotePath } from "$lib/utils/vaultTemplates";
  import {
    resolveJournalDailyHeroPath,
    resolveLastEditedNote,
  } from "$lib/utils/vaultNoteBridge";
  import { partitionWorkHub } from "$lib/utils/workHub";
  import {
    fetchPeerHomePreview,
    type PeerHomePreview,
  } from "$lib/utils/peerHomePreview";
  import {
    formatCardSubtitle,
    formatCardTitle,
  } from "$lib/utils/formatWork";
  import {
    homeContinueRows,
    homeNotesDateParts,
    peerInitials,
  } from "$lib/utils/homeContinue";
  import { isTauri } from "$lib/window";
  import type { WorkCard } from "$lib/types/workspace";

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
  const journalDailyPath = $derived(resolveJournalDailyHeroPath(vault.notes));
  const todayDailyPath = $derived(dailyNotePath());
  const lastEditedNote = $derived(resolveLastEditedNote(vault.notes));
  const partition = $derived(partitionWorkHub(workspace.cards));
  const living = $derived(partition.living);

  let peerPreview = $state<PeerHomePreview>({
    unreadTotal: 0,
    peerCount: 0,
    stripThreads: [],
    latestThread: null,
  });
  let peerPollTimer: ReturnType<typeof setInterval> | undefined;
  let sessionsHydrated = $state(false);

  const isOffline = $derived(health !== null && !health.ok);
  const isConnecting = $derived(health === null);

  const motionSummary = $derived(
    buildMotionSummary(motionColumnCounts(workspace.cards)),
  );

  const greeting = $derived.by(() => {
    const hour = new Date().getHours();
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
    if (peerPreview.unreadTotal > 0) {
      return peerPreview.unreadTotal === 1
        ? "1 message"
        : `${peerPreview.unreadTotal} messages`;
    }
    return "All clear";
  });

  const statusDotClass = $derived(
    health?.ok ? "bg-success-400" : health ? "bg-warning-400" : "bg-surface-500",
  );

  const notesDate = $derived(homeNotesDateParts());
  const notesWhisper = $derived.by(() => {
    if (lastEditedNote) {
      return vaultDisplayTitle(lastEditedNote.title ?? "", lastEditedNote.path);
    }
    if (vault.notes.some((note) => note.path === todayDailyPath)) return "Today’s journal";
    if (journalDailyPath) return "Recent journal";
    return "Start today";
  });

  const continueRows = $derived(homeContinueRows(chat.sessions, 3));
  const continueLead = $derived(continueRows[0] ?? null);
  const continueWhispers = $derived(continueRows.slice(1));

  const peerAvatarLabels = $derived(
    peerPreview.stripThreads.slice(0, 3).map((thread) => thread.label),
  );

  type HomeActivityBeat = {
    cardId: string | null;
    status: "done" | "need" | "motion";
    statusLabel: string;
    title: string;
    line: string;
  };

  const lastActivity = $derived.by((): HomeActivityBeat | null => {
    const pending = workspace.pendingAskCompletion;
    if (pending) {
      return {
        cardId: pending.jobId,
        status: "done",
        statusLabel: "Done",
        title: (pending.title ?? "").trim() || "Ask ready",
        line: "Tap to open the result",
      };
    }

    const blockedCard =
      workspace.cards.find((card) => card.column === "blocked") ??
      partition.stuck[0] ??
      null;
    if (blockedCard) {
      return beatFromCard(blockedCard, "need", "Needs you");
    }

    const motionCard = workspace.primaryInMotionCard();
    if (motionCard) {
      return beatFromCard(motionCard, "motion", "In motion");
    }

    const settled = partition.settled[0];
    if (settled) {
      return beatFromCard(settled, "done", "Done");
    }

    return null;
  });

  function beatFromCard(
    card: WorkCard,
    status: HomeActivityBeat["status"],
    statusLabel: string,
  ): HomeActivityBeat {
    return {
      cardId: card.id,
      status,
      statusLabel,
      title: formatCardTitle(card),
      line: formatCardSubtitle(card),
    };
  }

  let scrollEl: HTMLDivElement | undefined = $state();
  let activityEl: HTMLElement | undefined = $state();
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
    if (!isTauri()) {
      void chat.refreshSessions();
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
  });

  async function refreshPeerPreview() {
    peerPreview = await fetchPeerHomePreview();
  }

  function openMenu() {
    haptic("light");
    layout.openMobileDestinationsMenu();
  }

  function onStatusTap() {
    if (isOffline) {
      onOpenSettings();
      return;
    }
    if (lastActivity) {
      activityEl?.scrollIntoView({ behavior: "smooth", block: "start" });
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

  async function openContinue(sessionId: string) {
    haptic("light");
    await onOpenChat(sessionId);
  }

  async function openActivity() {
    if (!lastActivity?.cardId) return;
    haptic("light");
    // AskCompletionModal is hosted by MobileShell when pending — don't select a fake card id.
    if (workspace.pendingAskCompletion) return;
    await onSelectCard(lastActivity.cardId);
  }

  async function refresh() {
    await Promise.all([
      workspace.prefetchCardDetails(),
      refreshPeerPreview(),
      chat.refreshSessions(),
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
      <div class="mobile-home-topbar">
        <button
          type="button"
          class="mobile-home-menu-btn"
          aria-label="Open menu"
          onclick={openMenu}
        >
          <Menu size={18} strokeWidth={1.75} />
        </button>
        <WorkshopSwitcherCompact />
      </div>

      <div class="mobile-home-brand">
        <h1 class="mobile-home-wordmark">Medousa</h1>
        <p class="mobile-home-greeting-quiet">{greeting}</p>
        <button
          type="button"
          class="mobile-home-status-breath"
          onclick={onStatusTap}
        >
          <span
            class="mobile-alive-dot {living.length > 0
              ? 'mobile-alive-dot-active'
              : ''} {statusDotClass}"
            aria-hidden="true"
          ></span>
          <span class="truncate">{statusLine}</span>
        </button>
      </div>

      <div class="mobile-home-glance">
        <button type="button" class="mobile-home-glance-tile" onclick={openPeers}>
          <span class="mobile-home-glance-kicker">Peers</span>
          {#if peerPreview.unreadTotal > 0}
            <span class="mobile-home-glance-hero">{peerPreview.unreadTotal}</span>
            <span class="mobile-home-glance-sub">unread</span>
          {:else}
            <span class="mobile-home-glance-hero">
              {peerPreview.peerCount > 0 ? peerPreview.peerCount : "—"}
            </span>
            <span class="mobile-home-glance-sub">
              {peerPreview.peerCount > 0 ? "quiet" : "none yet"}
            </span>
          {/if}
          {#if peerAvatarLabels.length > 0}
            <div class="mobile-home-peer-avatars">
              {#each peerAvatarLabels as label (label)}
                <span class="mobile-home-peer-avatar-chip">{peerInitials(label)}</span>
              {/each}
            </div>
          {/if}
        </button>

        <button
          type="button"
          class="mobile-home-glance-tile"
          onclick={() => void openDailyNote()}
        >
          <span class="mobile-home-glance-kicker mobile-home-glance-kicker-accent">
            {notesDate.weekday}
          </span>
          <span class="mobile-home-glance-hero">{notesDate.day}</span>
          <span class="mobile-home-glance-title">Daily note</span>
          <span class="mobile-home-glance-whisper">{notesWhisper}</span>
        </button>
      </div>

      {#if continueLead}
        <div class="mobile-home-continue">
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

      {#if lastActivity}
        <button
          bind:this={activityEl}
          type="button"
          id="home-last-activity"
          class="mobile-home-activity"
          onclick={() => void openActivity()}
        >
          <p class="mobile-home-activity-kicker">Last activity</p>
          <p
            class="mobile-home-activity-status"
            class:mobile-home-activity-status-done={lastActivity.status === "done"}
            class:mobile-home-activity-status-need={lastActivity.status === "need"}
            class:mobile-home-activity-status-motion={lastActivity.status === "motion"}
          >
            {lastActivity.statusLabel}
          </p>
          <p class="mobile-home-activity-title">{lastActivity.title}</p>
          <p class="mobile-home-activity-line">{lastActivity.line}</p>
        </button>
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
