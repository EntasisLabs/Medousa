<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import PeerHomeStrip from "$lib/components/mobile/PeerHomeStrip.svelte";
  import WorkManifestCard from "$lib/components/work/WorkManifestCard.svelte";
  import WorkHubTrays from "$lib/components/work/WorkHubTrays.svelte";
  import { ArrowUp, Bell, BookOpen, Calendar, FileText, Users } from "@lucide/svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { haptic } from "$lib/haptics";
  import { recurring } from "$lib/stores/recurring.svelte";
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
  import type { ProvenanceChip } from "$lib/utils/workHub";
  import { partitionWorkHub } from "$lib/utils/workHub";
  import {
    fetchPeerHomePreview,
    peerHomeCardHint,
    type PeerHomePreview,
  } from "$lib/utils/peerHomePreview";
  import { isTauri } from "$lib/window";

  interface Props {
    health: DaemonHealth | null;
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenChat: () => void;
    onOpenNote: (path: string) => void | Promise<void>;
    onOpenSettings: () => void;
    onToggleActivity: () => void;
    showInlineAsk?: boolean;
  }

  let {
    health,
    onSelectCard,
    onOpenChat,
    onOpenNote,
    onOpenSettings,
    onToggleActivity,
    showInlineAsk = true,
  }: Props = $props();

  const blocked = $derived(workspace.needsAttentionCount());
  const inMotion = $derived(workspace.inMotionCount());
  const nextSchedule = $derived(recurring.soonestEnabled());
  const journalDailyPath = $derived(resolveJournalDailyHeroPath(vault.notes));
  const todayDailyPath = $derived(dailyNotePath());
  const lastEditedNote = $derived(resolveLastEditedNote(vault.notes));
  const automationCounts = $derived(automations.activeCount());
  const partition = $derived(partitionWorkHub(workspace.cards));
  const living = $derived(partition.living);

  // Work only earns space on Home when there is something live, waiting, or stuck.
  const hasMotion = $derived(
    living.length > 0 || blocked > 0 || partition.stuck.length > 0,
  );

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

  // One calm line instead of a competing hero + status pill + banner.
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

  let scrollEl: HTMLDivElement | undefined = $state();
  let inMotionEl: HTMLElement | undefined = $state();
  let pullY = $state(0);
  let refreshing = $state(false);
  let touchStartY = 0;
  let pulling = false;

  let toastMessage = $state<string | null>(null);
  let toastCardId = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const dailyShortcutHint = $derived.by(() => {
    if (vault.notes.some((note) => note.path === todayDailyPath)) return "Today";
    if (journalDailyPath) return "Recent journal";
    return "Start today";
  });

  const lastEditedTitle = $derived(
    lastEditedNote
      ? vaultDisplayTitle(lastEditedNote.title, lastEditedNote.path)
      : "Last edited",
  );

  const lastEditedHint = $derived.by(() => {
    if (!lastEditedNote) return "No notes yet";
    try {
      const date = new Date(lastEditedNote.modified_at_utc);
      const diffMs = Date.now() - date.getTime();
      const mins = Math.floor(diffMs / 60_000);
      if (mins < 1) return "Just now";
      if (mins < 60) return `${mins}m ago`;
      const hours = Math.floor(mins / 60);
      if (hours < 48) return `${hours}h ago`;
      return date.toLocaleDateString([], { month: "short", day: "numeric" });
    } catch {
      return "Recent";
    }
  });

  const automationsHint = $derived(
    automationCounts.total === 0
      ? "Scripts & schedules"
      : `${automationCounts.enabled}/${automationCounts.total} active`,
  );

  let peerPreview = $state<PeerHomePreview>({
    unreadTotal: 0,
    peerCount: 0,
    stripThreads: [],
    latestThread: null,
  });
  let peerPollTimer: ReturnType<typeof setInterval> | undefined;

  const peersHint = $derived(peerHomeCardHint(peerPreview));
  const showPeerStrip = $derived(peerPreview.stripThreads.length > 0);

  $effect(() => {
    void workspace.prefetchCardDetails();
  });

  onMount(() => {
    if (!isTauri()) return;
    void refreshPeerPreview();
    peerPollTimer = setInterval(() => {
      void refreshPeerPreview();
    }, 8000);
  });

  onDestroy(() => {
    if (peerPollTimer) clearInterval(peerPollTimer);
  });

  async function refreshPeerPreview() {
    peerPreview = await fetchPeerHomePreview();
  }

  function scrollToInMotion() {
    inMotionEl?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function openAsk() {
    haptic("light");
    layout.openAskSheet();
  }

  function onStatusTap() {
    if (isOffline) {
      onOpenSettings();
      return;
    }
    if (hasMotion) {
      scrollToInMotion();
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

  async function openLastEditedNote() {
    if (!lastEditedNote) return;
    haptic("light");
    await onOpenNote(lastEditedNote.path);
  }

  function openAutomations() {
    haptic("light");
    layout.openMore("automations");
  }

  function openPeers() {
    haptic("light");
    layout.openMore("peers");
  }

  async function refresh() {
    await Promise.all([workspace.prefetchCardDetails(), refreshPeerPreview()]);
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

  async function handleProvenance(chip: ProvenanceChip, cardId: string) {
    if (chip.kind === "vault" && chip.href) {
      onOpenNote(chip.href);
      return;
    }
    if (chip.kind === "chat") {
      const detail = workspace.cardDetailsCache.get(cardId);
      const sessionId = detail?.session_id?.trim();
      if (sessionId && sessionId !== chat.sessionId) {
        await chat.switchSession(sessionId);
      }
      onOpenChat();
      return;
    }
    void onSelectCard(cardId);
  }
</script>

<section class="mobile-home relative flex h-full min-h-0 flex-col">
  <div
    bind:this={scrollEl}
    class="mobile-pull-scroll min-h-0 flex-1 overflow-y-auto"
    role="region"
    aria-label="Home command center"
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

    <div class="px-5 pb-4 pt-3">
      <div class="flex items-center justify-end gap-2">
        <WorkshopSwitcherCompact />
        <ProfileSwitcherCompact />
        <button
          type="button"
          class="mobile-icon-btn shrink-0"
          aria-label="Activity"
          onclick={onToggleActivity}
        >
          <Bell size={20} strokeWidth={1.75} />
        </button>
      </div>

      <h1 class="mobile-home-greeting mt-6">{greeting}</h1>

      {#if showInlineAsk}
        <button
          type="button"
          class="mobile-home-ask mt-4"
          aria-label="Ask Medousa"
          onclick={openAsk}
        >
          <span class="mobile-home-ask-placeholder">Ask Medousa anything…</span>
          <span class="mobile-home-ask-send" aria-hidden="true">
            <ArrowUp size={18} strokeWidth={2.25} />
          </span>
        </button>
      {/if}

      <button
        type="button"
        class="mobile-home-statusline"
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

      <div class="mobile-home-cards mt-5">
        <button type="button" class="mobile-home-card" onclick={() => void openDailyNote()}>
          <BookOpen size={17} strokeWidth={1.75} class="mobile-home-card-icon" />
          <span class="mobile-home-card-label">Daily note</span>
          <span class="mobile-home-card-hint">{dailyShortcutHint}</span>
        </button>
        <button
          type="button"
          class="mobile-home-card"
          disabled={!lastEditedNote}
          onclick={() => void openLastEditedNote()}
        >
          <FileText size={17} strokeWidth={1.75} class="mobile-home-card-icon" />
          <span class="mobile-home-card-label">{lastEditedTitle}</span>
          <span class="mobile-home-card-hint">{lastEditedHint}</span>
        </button>
        <button type="button" class="mobile-home-card" onclick={openAutomations}>
          <Calendar size={17} strokeWidth={1.75} class="mobile-home-card-icon" />
          <span class="mobile-home-card-label">Automations</span>
          <span class="mobile-home-card-hint">{automationsHint}</span>
        </button>
        <button type="button" class="mobile-home-card" onclick={openPeers}>
          <Users size={17} strokeWidth={1.75} class="mobile-home-card-icon" />
          <span class="mobile-home-card-label">Peers</span>
          <span class="mobile-home-card-hint">{peersHint}</span>
        </button>
      </div>

      {#if showPeerStrip}
        <PeerHomeStrip threads={peerPreview.stripThreads} />
      {/if}
    </div>

    {#if hasMotion}
      <div
        bind:this={inMotionEl}
        id="home-in-motion"
        class="border-t border-surface-500/25 px-4 pb-8 pt-5"
      >
        <div class="work-hub-grid pb-2">
          {#each living as card (card.id)}
            <WorkManifestCard
              {card}
              detail={workspace.cardDetailsCache.get(card.id)}
              selected={workspace.selectedCardId === card.id}
              onSelect={(id) => void onSelectCard(id)}
              onProvenance={handleProvenance}
            />
          {/each}
        </div>

        <WorkHubTrays onSelectCard={onSelectCard} />

        {#if nextSchedule}
          <footer class="mt-6">
            <p class="mobile-pulse-whisper text-center">
              Next · {recurring.labelFor(nextSchedule)} ·
              {recurring.formatNextRun(nextSchedule.next_run_at_utc)}
            </p>
          </footer>
        {/if}
      </div>
    {/if}
  </div>

  <MobileToast
    message={toastMessage}
    actionLabel="Undo"
    onAction={undoCancel}
    onDismiss={dismissToast}
  />
</section>
