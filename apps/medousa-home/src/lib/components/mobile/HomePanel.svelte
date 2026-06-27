<script lang="ts">
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import WorkManifestCard from "$lib/components/work/WorkManifestCard.svelte";
  import WorkHubTrays from "$lib/components/work/WorkHubTrays.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import { Bell, BookOpen, Calendar, FileText, Plus } from "@lucide/svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { haptic } from "$lib/haptics";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { retryWorkspaceCard } from "$lib/daemon";
  import { buildPulsePresentation, motionColumnCounts } from "$lib/utils/mobilePulse";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { dailyNotePath } from "$lib/utils/vaultTemplates";
  import {
    journalDailyHeroTitle,
    resolveJournalDailyHeroPath,
    resolveLastEditedNote,
  } from "$lib/utils/vaultNoteBridge";
  import type { ProvenanceChip } from "$lib/utils/workHub";
  import { partitionWorkHub } from "$lib/utils/workHub";

  interface Props {
    health: DaemonHealth | null;
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenChat: () => void;
    onOpenNote: (path: string) => void | Promise<void>;
    onOpenSettings: () => void;
    onToggleActivity: () => void;
  }

  let {
    health,
    onSelectCard,
    onOpenChat,
    onOpenNote,
    onOpenSettings,
    onToggleActivity,
  }: Props = $props();

  const blocked = $derived(workspace.needsAttentionCount());
  const inMotion = $derived(workspace.inMotionCount());
  const primaryCard = $derived(workspace.primaryInMotionCard());
  const nextSchedule = $derived(recurring.soonestEnabled());
  const journalDailyPath = $derived(resolveJournalDailyHeroPath(vault.notes));
  const todayDailyPath = $derived(dailyNotePath());
  const lastEditedNote = $derived(resolveLastEditedNote(vault.notes));
  const automationCounts = $derived(automations.activeCount());
  const partition = $derived(partitionWorkHub(workspace.cards));
  const living = $derived(partition.living);

  const motionStatusLine = $derived.by(() => {
    if (living.length > 0) {
      return living.length === 1
        ? "1 in motion · pull to refresh"
        : `${living.length} in motion · pull to refresh`;
    }
    if (partition.stuck.length > 0) {
      return `${partition.stuck.length} stuck · pull to refresh`;
    }
    return "All clear · pull to refresh";
  });

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

  const pulse = $derived(
    buildPulsePresentation({
      healthOk: health === null ? null : health.ok,
      blocked,
      inMotion,
      primaryCard,
      motionCounts: motionColumnCounts(workspace.cards),
      journalDailyPath,
      journalDailyTitle: journalDailyPath
        ? journalDailyHeroTitle(journalDailyPath, vault.notes, vault.labelByPath())
        : null,
    }),
  );

  $effect(() => {
    void workspace.prefetchCardDetails();
  });

  function scrollToInMotion() {
    inMotionEl?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function runHeroAction() {
    switch (pulse.action.kind) {
      case "card":
        void onSelectCard(pulse.action.cardId);
        break;
      case "note":
        void onOpenNote(pulse.action.path);
        break;
      case "work":
        scrollToInMotion();
        break;
      case "chat":
        onOpenChat();
        break;
      case "settings":
        onOpenSettings();
        break;
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

  async function refresh() {
    await workspace.prefetchCardDetails();
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

  function showCancelToast(cardId: string, message: string) {
    if (toastTimer) clearTimeout(toastTimer);
    toastCardId = cardId;
    toastMessage = message || "Canceled";
    toastTimer = setTimeout(dismissToast, 5000);
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

    <div class="mobile-pulse px-5 pb-4 pt-3">
      <div class="flex items-center gap-3">
        <button
          type="button"
          class="mobile-pulse-status min-w-0 flex-1 text-left"
          onclick={() =>
            pulse.mood === "offline" ? onOpenSettings() : scrollToInMotion()}
        >
          <span
            class="mobile-alive-dot {pulse.alive && inMotion > 0
              ? 'mobile-alive-dot-active'
              : ''} {health?.ok ? 'bg-success-400' : health ? 'bg-warning-400' : 'bg-surface-500'}"
            aria-hidden="true"
          ></span>
          <span class="truncate text-xs text-surface-300">{pulse.statusLine}</span>
        </button>
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

      <div class="mt-8">
        <p class="mobile-pulse-eyebrow">{pulse.eyebrow}</p>
        <h1 class="mobile-pulse-headline mt-3">{pulse.headline}</h1>
        {#if pulse.subline}
          <p class="mobile-pulse-subline mt-3">{pulse.subline}</p>
        {/if}

        <button
          type="button"
          class="btn mobile-pulse-cta mt-6 w-full variant-filled-primary"
          onclick={runHeroAction}
        >
          {pulse.actionLabel}
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
        </div>

        {#if blocked > 0 && pulse.mood !== "waiting"}
          <button
            type="button"
            class="mt-4 w-full rounded-2xl border border-warning-500/30 bg-warning-500/8 px-4 py-3.5 text-left"
            onclick={scrollToInMotion}
          >
            <p class="text-sm font-medium text-warning-200">
              {blocked === 1 ? "1 needs you" : `${blocked} need you`}
            </p>
          </button>
        {/if}
      </div>
    </div>

    <div bind:this={inMotionEl} id="home-in-motion" class="border-t border-surface-500/25 px-4 pb-8">
      <p class="mobile-work-status py-3 text-center text-xs text-surface-400">
        {refreshing ? "Refreshing…" : motionStatusLine}
      </p>

      {#if living.length === 0}
        <EmptyState
          title="Nothing in motion"
          description="Tap + to ask Medousa something new."
        />
      {:else}
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
      {/if}

      <WorkHubTrays onSelectCard={onSelectCard} />

      <footer class="mt-6 space-y-2">
        {#if pulse.motionSummary && pulse.mood !== "quiet"}
          <p class="mobile-pulse-whisper text-center">{pulse.motionSummary}</p>
        {/if}
        {#if nextSchedule}
          <p class="mobile-pulse-whisper text-center">
            Next · {recurring.labelFor(nextSchedule)} ·
            {recurring.formatNextRun(nextSchedule.next_run_at_utc)}
          </p>
        {/if}
      </footer>
    </div>
  </div>

  <button
    type="button"
    class="mobile-home-fab"
    aria-label="New ask"
    onclick={() => layout.openAskSheet()}
  >
    <Plus size={24} strokeWidth={2} />
  </button>

  <MobileToast
    message={toastMessage}
    actionLabel="Undo"
    onAction={undoCancel}
    onDismiss={dismissToast}
  />
</section>
