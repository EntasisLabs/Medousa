<script lang="ts">
  import { Calendar, ChevronRight, Radio, Activity } from "@lucide/svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { messaging } from "$lib/stores/messaging.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import type { ProductConfigSummary } from "$lib/types/messaging";
  import { MESSAGING_CHANNELS } from "$lib/types/messaging";
  import { channelStatus } from "$lib/utils/channelStatus";

  interface Props {
    health: DaemonHealth | null;
    cronActiveCount: number;
    cronTotalCount: number;
    onOpenMessaging?: () => void;
    onOpenCron?: () => void;
    onOpenRuntime?: () => void;
  }

  let {
    health,
    cronActiveCount,
    cronTotalCount,
    onOpenMessaging,
    onOpenCron,
    onOpenRuntime,
  }: Props = $props();

  $effect(() => {
    void messaging.refresh();
    void recurring.refresh();
  });

  function messagingStatusLabel(summary: ProductConfigSummary | null, daemonOk: boolean): string {
    if (!summary) return "Loading…";
    const connected = MESSAGING_CHANNELS.filter(
      (channel) => channelStatus(channel.id, summary, daemonOk) === "connected",
    ).length;
    const ready = MESSAGING_CHANNELS.filter(
      (channel) => channelStatus(channel.id, summary, daemonOk) === "ready",
    ).length;
    if (connected > 0) {
      return `${connected} connected`;
    }
    if (ready > 0) {
      return `${ready} ready to connect`;
    }
    return "Needs setup";
  }

  const messagingStatus = $derived(
    messagingStatusLabel(messaging.summary, health?.ok ?? false),
  );
  const cronStatus = $derived(`${cronActiveCount}/${cronTotalCount} active`);
  const runtimeStatus = $derived(health?.ok ? "Connected" : "Offline");
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Integrations</h2>
    <p class="workshop-faint mt-1 text-sm">
      Channels, schedules, and live workshop health — each opens its own workspace.
    </p>
  </header>

  <ul class="settings-hub-list mt-5">
    {#if onOpenMessaging}
      <li>
        <button type="button" class="settings-hub-row" onclick={onOpenMessaging}>
          <span class="settings-hub-icon"><Radio size={16} strokeWidth={1.75} /></span>
          <span class="min-w-0 flex-1 text-left">
            <span class="block text-sm font-medium text-surface-100">Messaging</span>
            <span class="workshop-faint mt-0.5 block text-xs">{messagingStatus}</span>
          </span>
          <ChevronRight size={16} class="shrink-0 text-surface-500" />
        </button>
      </li>
    {/if}
    {#if onOpenCron}
      <li>
        <button type="button" class="settings-hub-row" onclick={onOpenCron}>
          <span class="settings-hub-icon"><Calendar size={16} strokeWidth={1.75} /></span>
          <span class="min-w-0 flex-1 text-left">
            <span class="block text-sm font-medium text-surface-100">Cron jobs</span>
            <span class="workshop-faint mt-0.5 block text-xs">{cronStatus}</span>
          </span>
          <ChevronRight size={16} class="shrink-0 text-surface-500" />
        </button>
      </li>
    {/if}
    {#if onOpenRuntime}
      <li>
        <button type="button" class="settings-hub-row" onclick={onOpenRuntime}>
          <span class="settings-hub-icon"><Activity size={16} strokeWidth={1.75} /></span>
          <span class="min-w-0 flex-1 text-left">
            <span class="block text-sm font-medium text-surface-100">Runtime telemetry</span>
            <span class="workshop-faint mt-0.5 block text-xs">{runtimeStatus}</span>
          </span>
          <ChevronRight size={16} class="shrink-0 text-surface-500" />
        </button>
      </li>
    {/if}
  </ul>
</section>
