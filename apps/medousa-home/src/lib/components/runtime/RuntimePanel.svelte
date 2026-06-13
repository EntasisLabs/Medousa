<script lang="ts">
  import WorkshopDefaultsPanel from "$lib/components/settings/WorkshopDefaultsPanel.svelte";
  import DaemonPortalChip from "$lib/components/chat/DaemonPortalChip.svelte";
  import { untrack } from "svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import type { DepthMode, RuntimeTab } from "$lib/types/runtime";

  interface Props {
    visible: boolean;
    inMotionCount: number;
    onOpenCron?: () => void;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, inMotionCount, onOpenCron, mobile = false, embedded = false }: Props =
    $props();

  let draftProvider = $state(runtime.provider);
  let draftModel = $state(runtime.model);
  let didInitialLoad = $state(false);

  const allTabs: { id: RuntimeTab; label: string }[] = [
    { id: "now", label: "Now" },
    { id: "jobs", label: "Jobs" },
    { id: "schedule", label: "Schedule" },
    { id: "delivery", label: "Delivery" },
    { id: "controls", label: "Controls" },
    { id: "workshop", label: "Workshop" },
    { id: "routing", label: "Routing" },
  ];

  const tabs = $derived(
    mobile
      ? allTabs.filter((tab) =>
          ["now", "jobs", "delivery", "controls", "workshop"].includes(tab.id),
        )
      : allTabs,
  );

  const streamingMessage = $derived(
    chat.messages.find((message) => message.streaming) ?? null,
  );

  const streamingStatusLine = $derived(
    streamingMessage
      ? visibleChatStatusLine(streamingMessage.statusLine, settings.showEngineDetailsInChat)
      : null,
  );

  $effect(() => {
    if (mobile && !tabs.some((tab) => tab.id === runtime.activeTab)) {
      runtime.activeTab = "now";
    }
  });

  $effect(() => {
    if (!visible) {
      didInitialLoad = false;
      return;
    }

    draftProvider = runtime.provider;
    draftModel = runtime.model;

    if (!didInitialLoad) {
      didInitialLoad = true;
      untrack(() => {
        void runtime.refresh();
        void runtime.refreshStageRoutes();
        void recurring.refresh();
      });
    }
  });

  function formatTimestamp(value: string | null | undefined): string {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString();
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
    <div class="flex items-start justify-between gap-4">
      {#if !embedded}
        <div>
          <h1 class="text-sm font-semibold text-surface-50">Runtime</h1>
          <p class="workshop-header-line mt-0.5">Live workshop telemetry</p>
        </div>
      {:else}
        <p class="workshop-faint text-xs">Live telemetry</p>
      {/if}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        disabled={runtime.loading}
        onclick={() => runtime.refresh()}
      >
        {runtime.loading ? "Refreshing…" : "Refresh"}
      </button>
    </div>

    <div class="workshop-tabs mt-3">
      {#each tabs as tab (tab.id)}
        <button
          type="button"
          class="workshop-tab {runtime.activeTab === tab.id ? 'workshop-tab-active' : ''}"
          onclick={() => (runtime.activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <p
      class="mt-2 min-h-[1.25rem] text-[11px] leading-snug {runtime.error
        ? 'text-warning-400'
        : 'text-transparent'}"
      title={runtime.errorDetail ?? runtime.error ?? undefined}
      aria-live="polite"
    >
      {runtime.error ?? "Telemetry nominal"}
    </p>
  </header>

  <div class="mobile-you-scroll flex-1 overflow-y-auto px-4 py-4">

    {#if runtime.activeTab === "now"}
      <dl class="workshop-telemetry">
        <div class="flex items-baseline">
          <dt>in motion</dt>
          <dd>{inMotionCount}</dd>
        </div>
        <div class="flex items-baseline">
          <dt>running</dt>
          <dd>{runtime.stats?.running_jobs ?? "—"}</dd>
        </div>
        <div class="flex items-baseline">
          <dt>queued</dt>
          <dd>{runtime.stats?.enqueued_jobs ?? "—"}</dd>
        </div>
        <div class="flex items-baseline">
          <dt>tick</dt>
          <dd>{formatTimestamp(runtime.stats?.last_tick_at_utc)}</dd>
        </div>
      </dl>

      <div class="py-3">
        {#if streamingMessage}
          <p class="text-sm text-surface-100">
            {formatTurnPhase(streamingMessage.phase ?? "streaming")}
          </p>
          {#if streamingStatusLine}
            <p class="workshop-faint mt-0.5">{streamingStatusLine}</p>
          {/if}
          {#if streamingMessage.tools?.length}
            <p class="mt-1 font-mono text-[10px] text-surface-500">
              {streamingMessage.tools.map((tool) => formatToolName(tool)).join(" · ")}
            </p>
          {/if}
        {:else if chat.liveStreamActive}
          <p class="text-sm text-surface-300">Starting turn…</p>
        {:else if chat.backgroundActivity > 0}
          <p class="text-sm text-surface-300">
            {chat.backgroundActivity === 1
              ? "1 turn in background"
              : `${chat.backgroundActivity} turns in background`}
          </p>
        {:else}
          <p class="workshop-faint">No active turn</p>
        {/if}
      </div>

      <p class="workshop-faint border-t border-surface-500/40 py-2.5 font-mono text-[11px]">
        {runtime.modelLabel()} · depth {runtime.depthMode}
      </p>
    {:else if runtime.activeTab === "jobs"}
      {#if runtime.stats}
        <dl class="workshop-telemetry flex-col !items-stretch gap-0 border-b-0">
          {#each [
            ["enqueued", runtime.stats.enqueued_jobs],
            ["running", runtime.stats.running_jobs],
            ["succeeded", runtime.stats.succeeded_jobs],
            ["failed", runtime.stats.failed_jobs],
            ["dead letter", runtime.stats.dead_letter_jobs],
            ["outbox", runtime.stats.pending_outbox_events],
            ["recurring", runtime.stats.recurring_definitions],
          ] as [label, value] (label)}
            <div class="flex items-baseline justify-between border-b border-surface-500/30 py-1.5">
              <dt>{label}</dt>
              <dd class="ml-0 text-sm">{value}</dd>
            </div>
          {/each}
        </dl>
        <p class="workshop-faint mt-4">
          Last tick {formatTimestamp(runtime.stats.last_tick_at_utc)}
        </p>
      {:else if runtime.loading}
        <p class="workshop-muted">Loading job stats…</p>
      {:else}
        <p class="workshop-muted">No stats yet — refresh to poll the daemon.</p>
      {/if}
    {:else if runtime.activeTab === "schedule"}
      <div class="space-y-3">
        <p class="workshop-muted text-sm">
          Cron jobs live in the dedicated Cron workspace — search, pause, and create from one list.
        </p>
        <p class="workshop-faint">
          {recurring.activeCount().enabled}/{recurring.activeCount().total} active
        </p>
        {#if onOpenCron}
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            onclick={onOpenCron}
          >
            Open Cron jobs
          </button>
        {/if}
      </div>
    {:else if runtime.activeTab === "delivery"}
      <div class="space-y-4">
        {#if runtime.delivery}
          <section class="workshop-inset p-4">
            <h2 class="workshop-section-title">Outbox</h2>
            <dl class="mt-3 space-y-2 text-xs">
              <div class="grid grid-cols-[8rem_1fr] gap-2">
                <dt class="workshop-label">Endpoint</dt>
                <dd class="font-mono text-surface-300">{runtime.delivery.endpoint_id}</dd>
              </div>
              <div class="grid grid-cols-[8rem_1fr] gap-2">
                <dt class="workshop-label">Target</dt>
                <dd class="break-all font-mono text-surface-300">
                  {runtime.delivery.endpoint_target || "—"}
                </dd>
              </div>
              <div class="grid grid-cols-[8rem_1fr] gap-2">
                <dt class="workshop-label">Pending</dt>
                <dd class="font-mono text-surface-300">
                  {runtime.delivery.pending_job_deliveries}
                </dd>
              </div>
              <div class="grid grid-cols-[8rem_1fr] gap-2">
                <dt class="workshop-label">Auth configured</dt>
                <dd class="text-surface-300">
                  {runtime.delivery.deliver_webhook_auth_configured ? "yes" : "no"}
                </dd>
              </div>
              <div class="grid grid-cols-[8rem_1fr] gap-2">
                <dt class="workshop-label">Last delivery</dt>
                <dd class="font-mono text-surface-300">
                  {formatTimestamp(runtime.delivery.last_delivery_at_utc)}
                </dd>
              </div>
            </dl>
          </section>
        {/if}

        {#if runtime.continuations}
          <section class="workshop-inset p-4">
            <h2 class="workshop-section-title">Continuations</h2>
            <dl class="mt-3 grid grid-cols-2 gap-3 text-xs">
              <div>
                <dt class="workshop-label">Pending</dt>
                <dd class="mt-0.5 font-mono text-surface-200">
                  {runtime.continuations.pending_count}
                </dd>
              </div>
              <div>
                <dt class="workshop-label">Resumed</dt>
                <dd class="mt-0.5 font-mono text-surface-200">
                  {runtime.continuations.resumed_count}
                </dd>
              </div>
              <div>
                <dt class="workshop-label">Consumed</dt>
                <dd class="mt-0.5 font-mono text-surface-200">
                  {runtime.continuations.consumed_count}
                </dd>
              </div>
              <div>
                <dt class="workshop-label">DLQ pending</dt>
                <dd class="mt-0.5 font-mono text-surface-200">
                  {runtime.continuations.dead_letter_pending_count}
                </dd>
              </div>
            </dl>
            {#if runtime.continuations.last_resume_at_utc}
              <p class="workshop-faint mt-3">
                Last resume {formatTimestamp(runtime.continuations.last_resume_at_utc)}
              </p>
            {/if}
          </section>
        {/if}
      </div>
    {:else if runtime.activeTab === "controls"}
      <section class="max-w-xl space-y-4">
        {#if mobile}
          <div class="workshop-inset space-y-3 p-4">
            <DaemonPortalChip />
            <p class="workshop-faint text-xs">
              Model and stage routing are configured on your Mac. This phone sends turns to the
              daemon — change Voice and Reach in Mac Settings, or edit
              <span class="font-mono text-surface-400">tui_defaults.json</span> there.
            </p>
          </div>
        {:else}
          <p class="workshop-faint">
            Session override only — tweaks the next turns without rewriting your charter. Voice lives
            in Settings → Voice; tools and delegation in Settings → Reach.
          </p>
        {/if}

        {#if !mobile}
        <div class="workshop-inset p-4">
          <h2 class="text-sm font-semibold text-surface-100">Model</h2>
          <div class="mt-4 grid gap-3 sm:grid-cols-2">
            <label class="workshop-label block" for="runtime-provider">
              Provider
            </label>
            <label class="workshop-label block" for="runtime-model">
              Model
            </label>
            <input
              id="runtime-provider"
              class="input"
              bind:value={draftProvider}
              placeholder="ollama"
            />
            <input
              id="runtime-model"
              class="input"
              bind:value={draftModel}
              placeholder="qwen2.5:7b"
            />
          </div>
          <button
            type="button"
            class="btn variant-filled-primary mt-4"
            disabled={runtime.savingControls || !draftProvider.trim() || !draftModel.trim()}
            onclick={() => runtime.applyModel(draftProvider, draftModel)}
          >
            {runtime.savingControls ? "Applying…" : "Apply model"}
          </button>
        </div>
        {/if}

        {#if mobile}
        <div class="workshop-inset p-4">
          <h2 class="text-sm font-semibold text-surface-100">Active model</h2>
          <p class="mt-2 font-mono text-sm text-surface-200">{runtime.modelLabel()}</p>
          <p class="workshop-faint mt-1 text-xs">Read from Mac daemon after connect</p>
        </div>
        {/if}

        <div class="workshop-inset p-4">
          <h2 class="text-sm font-semibold text-surface-100">Response depth</h2>
          <div class="mt-4 flex flex-wrap gap-2">
            {#each ["concise", "standard", "deep"] as mode (mode)}
              <button
                type="button"
                class="rounded-container-token px-3 py-2 text-sm transition {runtime.depthMode ===
                mode
                  ? 'bg-primary-500/20 font-medium text-primary-200'
                  : 'bg-surface-800 text-surface-300 hover:text-surface-100'}"
                disabled={runtime.savingControls}
                onclick={() => runtime.setDepthMode(mode as DepthMode)}
              >
                {mode}
              </button>
            {/each}
          </div>
          <p class="workshop-faint mt-3">{runtime.depthHint()}</p>
        </div>

        {#if runtime.controlsMessage}
          <p
            class="text-xs {runtime.controlsMessage.startsWith('✓') ||
            runtime.controlsMessage.includes('set')
              ? 'text-success-400'
              : 'text-warning-400'}"
          >
            {runtime.controlsMessage}
          </p>
        {/if}
      </section>
    {:else if runtime.activeTab === "workshop"}
      <div class="mb-3 rounded-container-token border border-surface-500/35 bg-surface-900/30 px-3 py-2">
        <p class="text-xs text-surface-300">
          <span class="font-medium text-surface-200">Terminal mirror.</span>
          Day-to-day charter is in Settings (Memory, Voice, Reach). Edit everything here when you
          need the full matrix — verifier thresholds, secrets, all tool round limits.
        </p>
      </div>
      <WorkshopDefaultsPanel visible={visible} {mobile} embedded />
    {:else}
      <section>
        <p class="workshop-faint">
          Live view of configured stage routing — edit posture in Settings → Reach; change
          individual specialists in Workshop → Specialists.
        </p>
        <div class="mt-4 overflow-x-auto">
          <table class="w-full min-w-[32rem] text-left text-xs">
            <thead>
              <tr class="workshop-label border-b border-surface-500/45">
                <th class="py-2 pr-3 font-medium">Role</th>
                <th class="py-2 pr-3 font-medium">Target</th>
                <th class="py-2 pr-3 font-medium">Policy</th>
                <th class="py-2 font-medium">Fallback</th>
              </tr>
            </thead>
            <tbody>
              {#each runtime.stageRoutes() as route (route.role)}
                <tr class="border-b border-surface-500/10">
                  <td class="py-2.5 pr-3 font-medium text-surface-200">{route.role}</td>
                  <td class="py-2.5 pr-3 font-mono text-surface-300">
                    {route.provider}:{route.model}
                  </td>
                  <td class="py-2.5 pr-3 text-surface-300">{route.policy_profile}</td>
                  <td class="workshop-faint py-2.5">{route.fallback_chain.join(", ")}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    {/if}
  </div>
</section>
