<script lang="ts">
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { Bell, BookOpen, Calendar, FileText } from "@lucide/svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { haptic } from "$lib/haptics";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { buildPulsePresentation, motionColumnCounts } from "$lib/utils/mobilePulse";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { dailyNotePath } from "$lib/utils/vaultTemplates";
  import {
    journalDailyHeroTitle,
    resolveJournalDailyHeroPath,
    resolveLastEditedNote,
  } from "$lib/utils/vaultNoteBridge";

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
        ? journalDailyHeroTitle(
            journalDailyPath,
            vault.notes,
            vault.labelByPath(),
          )
        : null,
    }),
  );

  function runHeroAction() {
    switch (pulse.action.kind) {
      case "card":
        void onSelectCard(pulse.action.cardId);
        break;
      case "note":
        void onOpenNote(pulse.action.path);
        break;
      case "work":
        layout.setMobileTab("work");
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
    layout.openYou("automations");
  }
</script>

<section class="mobile-pulse flex flex-1 flex-col px-5 pb-8 pt-3">
  <div class="flex items-center gap-3">
    <button
      type="button"
      class="mobile-pulse-status min-w-0 flex-1 text-left"
      onclick={() => (pulse.mood === "offline" ? onOpenSettings() : layout.setMobileTab("work"))}
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

  <div class="mt-10 flex flex-1 flex-col justify-center">
    <p class="mobile-pulse-eyebrow">{pulse.eyebrow}</p>
    <h1 class="mobile-pulse-headline mt-3">{pulse.headline}</h1>
    {#if pulse.subline}
      <p class="mobile-pulse-subline mt-3">{pulse.subline}</p>
    {/if}

    <button
      type="button"
      class="btn mobile-pulse-cta mt-8 w-full variant-filled-primary"
      onclick={runHeroAction}
    >
      {pulse.actionLabel}
    </button>

    <div class="mobile-pulse-shortcuts">
      <button type="button" class="mobile-pulse-shortcut" onclick={() => void openDailyNote()}>
        <BookOpen size={17} strokeWidth={1.75} class="mobile-pulse-shortcut-icon" />
        <span class="mobile-pulse-shortcut-label">Daily note</span>
        <span class="mobile-pulse-shortcut-hint">{dailyShortcutHint}</span>
      </button>
      <button
        type="button"
        class="mobile-pulse-shortcut"
        disabled={!lastEditedNote}
        onclick={() => void openLastEditedNote()}
      >
        <FileText size={17} strokeWidth={1.75} class="mobile-pulse-shortcut-icon" />
        <span class="mobile-pulse-shortcut-label">{lastEditedTitle}</span>
        <span class="mobile-pulse-shortcut-hint">{lastEditedHint}</span>
      </button>
      <button type="button" class="mobile-pulse-shortcut" onclick={openAutomations}>
        <Calendar size={17} strokeWidth={1.75} class="mobile-pulse-shortcut-icon" />
        <span class="mobile-pulse-shortcut-label">Automations</span>
        <span class="mobile-pulse-shortcut-hint">{automationsHint}</span>
      </button>
    </div>

    {#if blocked > 0 && pulse.mood !== "waiting"}
      <button
        type="button"
        class="mt-4 w-full rounded-2xl border border-warning-500/30 bg-warning-500/8 px-4 py-3.5 text-left"
        onclick={() => layout.setMobileTab("work")}
      >
        <p class="text-sm font-medium text-warning-200">
          {blocked === 1 ? "1 needs you" : `${blocked} need you`}
        </p>
      </button>
    {/if}
  </div>

  <footer class="mt-auto space-y-2 pt-6">
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
</section>
