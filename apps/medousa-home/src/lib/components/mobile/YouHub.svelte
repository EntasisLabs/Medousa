<script lang="ts">
  import {
    Activity,
    BookOpen,
    Calendar,
    ChevronLeft,
    ChevronRight,
    Radio,
    Settings,
    SlidersHorizontal,
    Sparkles,
  } from "@lucide/svelte";
  import CronPanel from "$lib/components/cron/CronPanel.svelte";
  import MobileLibraryPanel from "$lib/components/mobile/MobileLibraryPanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import WorkshopDefaultsPanel from "$lib/components/settings/WorkshopDefaultsPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { cronDraft } from "$lib/stores/cron.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { YOU_DESTINATIONS, type YouDestination } from "$lib/types/mobile";
  import type { DaemonHealth } from "$lib/daemon";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { Component } from "svelte";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    revision: number;
    onOpenChat: () => void;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, health, revision, onOpenChat, onDaemonHealth }: Props = $props();

  const destinationIcons: Record<
    Exclude<YouDestination, "hub">,
    Component
  > = {
    library: BookOpen,
    skills: Sparkles,
    cron: Calendar,
    messaging: Radio,
    settings: Settings,
    advanced: SlidersHorizontal,
    runtime: Activity,
  };

  function openDestination(id: Exclude<YouDestination, "hub">) {
    layout.openYou(id);
  }

  const activeLabel = $derived(
    YOU_DESTINATIONS.find((dest) => dest.id === layout.youDestination)?.label ??
      layout.youDestination,
  );
</script>

<div class="flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  {#if layout.youDestination === "hub"}
    <header class="mobile-you-header">
      <h1 class="text-base font-semibold text-surface-50">You</h1>
      <p class="workshop-faint mt-0.5">Everything else — one tap away</p>
    </header>
    <ul class="mobile-you-scroll flex-1 overflow-y-auto px-3 py-2">
      {#each YOU_DESTINATIONS as dest (dest.id)}
        {@const Icon = destinationIcons[dest.id]}
        <li class="mb-1">
          <button
            type="button"
            class="mobile-you-destination"
            onclick={() => openDestination(dest.id)}
          >
            <span class="mobile-you-destination-icon">
              <Icon size={18} strokeWidth={1.75} />
            </span>
            <span class="min-w-0 flex-1">
              <p class="font-medium text-surface-100">{dest.label}</p>
              <p class="workshop-faint mt-0.5 text-xs">{dest.hint}</p>
            </span>
            <ChevronRight size={18} class="shrink-0 text-surface-500" />
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <header class="mobile-you-subheader flex items-center gap-2">
      <button
        type="button"
        class="mobile-icon-btn shrink-0"
        aria-label="Back to You hub"
        onclick={() => layout.backToYouHub()}
      >
        <ChevronLeft size={20} strokeWidth={1.75} />
      </button>
      <h1 class="min-w-0 truncate text-sm font-semibold">{activeLabel}</h1>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden">
      {#if layout.youDestination === "library"}
        <MobileLibraryPanel visible={true} />
      {:else if layout.youDestination === "skills"}
        <SkillsPanel
          visible={true}
          embedded={true}
          mobile={true}
          {onOpenChat}
          onScheduleSkill={(entry) => {
            cronDraft.openCreate({
              prompt: `Run ${entry.name} on schedule`,
              cron_expr: "0 9 * * *",
              manuscript_id: entry.id,
            });
            layout.openYou("cron");
          }}
        />
      {:else if layout.youDestination === "cron"}
        <CronPanel visible={true} embedded={true} mobile={true} />
      {:else if layout.youDestination === "messaging"}
        <MessagingPanel visible={true} {health} embedded={true} mobile={true} />
      {:else if layout.youDestination === "settings"}
        <SettingsPanel
          visible={true}
          embedded={true}
          mobile={true}
          {revision}
          {health}
          onOpenRuntime={() => layout.openYou("runtime")}
          onOpenAdvanced={() => layout.openYou("advanced")}
          {onDaemonHealth}
        />
      {:else if layout.youDestination === "advanced"}
        <div class="mobile-you-scroll h-full overflow-y-auto px-4 py-4">
          <WorkshopDefaultsPanel visible={true} mobile={true} />
        </div>
      {:else if layout.youDestination === "runtime"}
        <RuntimePanel
          visible={true}
          embedded={true}
          mobile={true}
          inMotionCount={workspace.inMotionCount()}
          onOpenCron={() => layout.openYou("cron")}
        />
      {/if}
    </div>
  {/if}
</div>
