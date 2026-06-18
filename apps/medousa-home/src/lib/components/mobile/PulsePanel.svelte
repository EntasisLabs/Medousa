<script lang="ts">
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import { Bell } from "@lucide/svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { buildPulsePresentation, motionColumnCounts } from "$lib/utils/mobilePulse";
  import {
    journalDailyHeroTitle,
    resolveJournalDailyHeroPath,
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
