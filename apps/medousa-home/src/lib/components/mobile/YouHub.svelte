<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import CronPanel from "$lib/components/cron/CronPanel.svelte";
  import LibraryPanel from "$lib/components/vault/LibraryPanel.svelte";
  import MessagingPanel from "$lib/components/messaging/MessagingPanel.svelte";
  import RuntimePanel from "$lib/components/runtime/RuntimePanel.svelte";
  import SettingsPanel from "$lib/components/layout/SettingsPanel.svelte";
  import SkillsPanel from "$lib/components/skills/SkillsPanel.svelte";
  import { cronDraft } from "$lib/stores/cron.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { YOU_DESTINATIONS, type YouDestination } from "$lib/types/mobile";
  import type { DaemonHealth } from "$lib/daemon";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    visible: boolean;
    health: DaemonHealth | null;
    revision: number;
    onOpenChat: () => void;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, health, revision, onOpenChat, onDaemonHealth }: Props = $props();

  function openDestination(id: Exclude<YouDestination, "hub">) {
    layout.openYou(id);
  }
</script>

<div class="flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}">
  {#if layout.youDestination === "hub"}
    <header class="workshop-header">
      <h1 class="text-sm font-semibold">You</h1>
      <p class="workshop-faint">Everything else — one tap away</p>
    </header>
    <ul class="flex-1 overflow-y-auto px-4 py-3">
      {#each YOU_DESTINATIONS as dest (dest.id)}
        <li class="border-b border-surface-500/30 last:border-0">
          <button
            type="button"
            class="flex w-full items-center justify-between gap-3 py-4 text-left"
            onclick={() => openDestination(dest.id)}
          >
            <div>
              <p class="font-medium text-surface-100">{dest.label}</p>
              <p class="workshop-faint mt-0.5">{dest.hint}</p>
            </div>
            <ChevronRight size={18} class="shrink-0 text-surface-500" />
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <header class="workshop-header flex items-center gap-3">
      <button
        type="button"
        class="workshop-text-action shrink-0 text-sm"
        onclick={() => layout.backToYouHub()}
      >
        ← You
      </button>
      <h1 class="text-sm font-semibold capitalize">
        {YOU_DESTINATIONS.find((d) => d.id === layout.youDestination)?.label ??
          layout.youDestination}
      </h1>
    </header>
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if layout.youDestination === "library"}
        <LibraryPanel visible={true} />
      {:else if layout.youDestination === "skills"}
        <SkillsPanel
          visible={true}
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
        <CronPanel visible={true} />
      {:else if layout.youDestination === "messaging"}
        <MessagingPanel visible={true} {health} />
      {:else if layout.youDestination === "settings"}
        <SettingsPanel
          visible={true}
          {revision}
          {health}
          onOpenRuntime={() => layout.openYou("runtime")}
          onOpenMessaging={() => layout.openYou("messaging")}
          onOpenCron={() => layout.openYou("cron")}
          {onDaemonHealth}
        />
      {:else if layout.youDestination === "runtime"}
        <RuntimePanel
          visible={true}
          inMotionCount={workspace.inMotionCount()}
          onOpenCron={() => layout.openYou("cron")}
        />
      {/if}
    </div>
  {/if}
</div>
