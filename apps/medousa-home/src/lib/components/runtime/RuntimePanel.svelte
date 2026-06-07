<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import type { DepthMode, RuntimeTab } from "$lib/types/runtime";

  interface Props {
    visible: boolean;
    inMotionCount: number;
  }

  let { visible, inMotionCount }: Props = $props();

  let draftProvider = $state(runtime.provider);
  let draftModel = $state(runtime.model);

  const tabs: { id: RuntimeTab; label: string }[] = [
    { id: "now", label: "Now" },
    { id: "jobs", label: "Jobs" },
    { id: "schedule", label: "Schedule" },
    { id: "delivery", label: "Delivery" },
    { id: "controls", label: "Controls" },
    { id: "routing", label: "Routing" },
  ];

  const streamingMessage = $derived(
    chat.messages.find((message) => message.streaming) ?? null,
  );

  $effect(() => {
    if (visible) {
      draftProvider = runtime.provider;
      draftModel = runtime.model;
      void runtime.refresh();
      void runtime.refreshStageRoutes();
      void recurring.refresh();
    }
  });

  function formatTimestamp(value: string | null | undefined): string {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString();
  }
</script>

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header">
    <div class="flex items-start justify-between gap-4">
      <div>
        <h1 class="text-base font-semibold text-surface-50">Runtime</h1>
        <p class="text-xs text-surface-300">
          What the workshop is doing right now
        </p>
      </div>
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        disabled={runtime.loading}
        onclick={() => runtime.refresh()}
      >
        {runtime.loading ? "Refreshing…" : "Refresh"}
      </button>
    </div>

    <div class="mt-4 flex flex-wrap gap-1">
      {#each tabs as tab (tab.id)}
        <button
          type="button"
          class="rounded-container-token px-3 py-1.5 text-xs transition {runtime.activeTab ===
          tab.id
            ? 'bg-primary-500/20 font-medium text-primary-200'
            : 'text-surface-300 hover:bg-surface-700/80 hover:text-surface-50'}"
          onclick={() => (runtime.activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </header>

  <div class="flex-1 overflow-y-auto px-5 py-4">
    {#if runtime.error}
      <p class="mb-4 text-sm text-error-400">{runtime.error}</p>
    {/if}

    {#if runtime.activeTab === "now"}
      <div class="space-y-4">
        <section class="workshop-inset p-4">
          <h2 class="workshop-section-title">Active turn</h2>
          {#if streamingMessage}
            <p class="mt-2 text-sm text-surface-100">
              {formatTurnPhase(streamingMessage.phase ?? "streaming")}
            </p>
            {#if streamingMessage.statusLine}
              <p class="workshop-faint mt-1">{streamingMessage.statusLine}</p>
            {/if}
            {#if streamingMessage.tools?.length}
              <div class="mt-3 flex flex-wrap gap-1.5">
                {#each streamingMessage.tools as tool (tool)}
                  <span
                    class="rounded-token bg-surface-800 px-2 py-0.5 text-[11px] text-surface-300"
                  >
                    {formatToolName(tool)}
                  </span>
                {/each}
              </div>
            {/if}
          {:else if chat.isStreaming}
            <p class="mt-2 text-sm text-surface-300">Starting turn…</p>
          {:else}
            <p class="workshop-muted mt-2">No active turn</p>
          {/if}
        </section>

        <section class="workshop-inset p-4">
          <h2 class="workshop-section-title">Workshop pulse</h2>
          <dl class="mt-3 grid grid-cols-2 gap-3 text-xs">
            <div>
              <dt class="workshop-label">In motion</dt>
              <dd class="mt-0.5 font-mono text-surface-200">{inMotionCount}</dd>
            </div>
            <div>
              <dt class="workshop-label">Running jobs</dt>
              <dd class="mt-0.5 font-mono text-surface-200">
                {runtime.stats?.running_jobs ?? "—"}
              </dd>
            </div>
            <div>
              <dt class="workshop-label">Queued</dt>
              <dd class="mt-0.5 font-mono text-surface-200">
                {runtime.stats?.enqueued_jobs ?? "—"}
              </dd>
            </div>
            <div>
              <dt class="workshop-label">Last scheduler tick</dt>
              <dd class="mt-0.5 font-mono text-surface-200">
                {formatTimestamp(runtime.stats?.last_tick_at_utc)}
              </dd>
            </div>
          </dl>
        </section>

        <section class="workshop-inset p-4">
          <h2 class="workshop-section-title">Current config</h2>
          <p class="mt-2 font-mono text-sm text-surface-200">{runtime.modelLabel()}</p>
          <p class="workshop-faint mt-1">
            Depth {runtime.depthMode} · {runtime.depthHint()}
          </p>
        </section>
      </div>
    {:else if runtime.activeTab === "jobs"}
      {#if runtime.stats}
        <dl class="grid gap-3 sm:grid-cols-2">
          {#each [
            ["Enqueued", runtime.stats.enqueued_jobs],
            ["Running", runtime.stats.running_jobs],
            ["Succeeded", runtime.stats.succeeded_jobs],
            ["Failed", runtime.stats.failed_jobs],
            ["Dead letter", runtime.stats.dead_letter_jobs],
            ["Pending outbox", runtime.stats.pending_outbox_events],
            ["Recurring", runtime.stats.recurring_definitions],
          ] as [label, value] (label)}
            <div
              class="workshop-inset p-4"
            >
              <dt class="workshop-label">{label}</dt>
              <dd class="mt-1 font-mono text-lg text-surface-100">{value}</dd>
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
      {#if recurring.loading}
        <p class="workshop-muted">Loading schedules…</p>
      {:else if recurring.error}
        <p class="text-sm text-error-400">{recurring.error}</p>
      {:else if recurring.definitions.length === 0}
        <p class="workshop-muted">
          No recurring schedules yet. Schedule a skill from Skills or register via chat.
        </p>
      {:else}
        <ul class="space-y-3">
          {#each recurring.definitions as entry (entry.recurring_id)}
            <li class="workshop-inset p-4">
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <p class="font-medium text-surface-100">
                    {recurring.labelFor(entry)}
                  </p>
                  <p class="workshop-faint mt-1 font-mono">
                    {entry.cron_expr} · {entry.timezone}
                  </p>
                  {#if entry.prompt_excerpt && entry.manuscript_id}
                    <p class="workshop-faint mt-2">{entry.prompt_excerpt}</p>
                  {/if}
                </div>
                <span
                  class="badge shrink-0 {entry.enabled
                    ? 'variant-soft-primary'
                    : 'variant-soft-surface'}"
                >
                  {entry.enabled ? "enabled" : "paused"}
                </span>
              </div>
              <dl class="mt-3 grid grid-cols-2 gap-2 text-xs">
                <div>
                  <dt class="workshop-label">Next run</dt>
                  <dd class="mt-0.5 text-surface-200">
                    {recurring.formatNextRun(entry.next_run_at_utc)}
                  </dd>
                </div>
                <div>
                  <dt class="workshop-label">Last run</dt>
                  <dd class="mt-0.5 text-surface-200">
                    {entry.last_run_at_utc
                      ? recurring.formatNextRun(entry.last_run_at_utc)
                      : "—"}
                  </dd>
                </div>
              </dl>
            </li>
          {/each}
        </ul>
      {/if}
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
        <p class="workshop-faint">
          Model and depth apply to the next chat turn from Home.
        </p>

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
    {:else}
      <section>
        <p class="workshop-faint">
          Per-stage provider and model routing (read-only).
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
