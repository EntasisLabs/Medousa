<script lang="ts">
  import { CircleAlert, CircleCheck, Clock3 } from "@lucide/svelte";
  import MobileRuntimeDock from "$lib/components/mobile/MobileRuntimeDock.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import { formatWorkshopUpdatedAt } from "$lib/utils/runtimePresentation";

  interface Props {
    inMotionCount: number;
  }

  let { inMotionCount }: Props = $props();

  const streamingMessage = $derived(
    chat.messages.find((message) => message.streaming) ?? null,
  );

  const streamingStatusLine = $derived(
    streamingMessage
      ? visibleChatStatusLine(streamingMessage.statusLine, settings.showEngineDetailsInChat)
      : null,
  );

  const runningCount = $derived(
    Math.max(
      inMotionCount,
      runtime.stats?.running_jobs ?? 0,
      runtime.stats?.active_turn_executions ?? 0,
      chat.liveStreamActive ? 1 : 0,
      chat.backgroundActivity,
    ),
  );
  const waitingCount = $derived(runtime.stats?.enqueued_jobs ?? 0);
  const runningJobCount = $derived(runtime.stats?.running_jobs ?? 0);
  const hasTelemetry = $derived(
    Boolean(runtime.stats || runtime.delivery || runtime.continuations),
  );
  const failedCount = $derived(runtime.stats?.failed_jobs ?? 0);
  const needsReviewCount = $derived(runtime.stats?.dead_letter_jobs ?? 0);
  const completedCount = $derived(runtime.stats?.succeeded_jobs ?? 0);

  const deliveryWaitingCount = $derived(
    Math.max(
      runtime.delivery?.pending_job_deliveries ?? 0,
      runtime.stats?.pending_outbox_events ?? 0,
    ),
  );
  const followUpWaitingCount = $derived(runtime.continuations?.pending_count ?? 0);
  const deliveryNeedsReviewCount = $derived(
    runtime.continuations?.dead_letter_pending_count ?? 0,
  );
  const hasFollowUpHistory = $derived(
    Boolean(
      runtime.continuations &&
        (runtime.continuations.pending_count > 0 ||
          runtime.continuations.resumed_count > 0 ||
          runtime.continuations.consumed_count > 0 ||
          runtime.continuations.dead_letter_pending_count > 0),
    ),
  );

  function countLabel(count: number, singular: string, plural = `${singular}s`): string {
    return `${count} ${count === 1 ? singular : plural}`;
  }
</script>

<div class="runtime-mobile-shell">
  <main class="mobile-you-scroll runtime-mobile-body">
    {#if runtime.error && hasTelemetry}
      <div class="runtime-mobile-notice" role="status" title={runtime.errorDetail ?? runtime.error}>
        <CircleAlert size={16} strokeWidth={1.8} aria-hidden="true" />
        <span>Some live details couldn’t be refreshed.</span>
      </div>
    {/if}

    {#if runtime.activeTab === "now"}
      <section class="runtime-mobile-section runtime-mobile-section-leading" aria-label="Current activity">
        <div class="runtime-mobile-card runtime-mobile-current-card">
          {#if runtime.loading && !runtime.stats && runningCount === 0}
            <span class="runtime-mobile-card-icon runtime-mobile-card-icon-active" aria-hidden="true">
              <Clock3 size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">Checking activity…</p>
            </div>
          {:else if runtime.error && !runtime.stats && runningCount === 0}
            <span class="runtime-mobile-card-icon runtime-mobile-card-icon-warning" aria-hidden="true">
              <CircleAlert size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">Activity is unavailable</p>
              <p class="runtime-mobile-card-detail">Try refreshing in a moment.</p>
            </div>
          {:else if streamingMessage}
            <span class="runtime-mobile-card-icon runtime-mobile-card-icon-active" aria-hidden="true">
              <Clock3 size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">
                {formatTurnPhase(streamingMessage.phase ?? "streaming")}
              </p>
              {#if streamingStatusLine}
                <p class="runtime-mobile-card-detail">{streamingStatusLine}</p>
              {/if}
              {#if streamingMessage.tools?.length}
                <p class="runtime-mobile-card-meta">
                  {streamingMessage.tools.map((tool) => formatToolName(tool)).join(" · ")}
                </p>
              {/if}
            </div>
          {:else if chat.liveStreamActive}
            <span class="runtime-mobile-card-icon runtime-mobile-card-icon-active" aria-hidden="true">
              <Clock3 size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">Starting work…</p>
              <p class="runtime-mobile-card-detail">Getting everything ready.</p>
            </div>
          {:else if chat.backgroundActivity > 0}
            <span class="runtime-mobile-card-icon runtime-mobile-card-icon-active" aria-hidden="true">
              <Clock3 size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">Working in the background</p>
              <p class="runtime-mobile-card-detail">
                {countLabel(chat.backgroundActivity, "turn")} still in motion.
              </p>
            </div>
          {:else if runningCount > 0}
            <span class="runtime-mobile-card-icon runtime-mobile-card-icon-active" aria-hidden="true">
              <Clock3 size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">Work is in motion</p>
              <p class="runtime-mobile-card-detail">
                {countLabel(runningCount, "thing")} running right now.
              </p>
            </div>
          {:else}
            <span class="runtime-mobile-card-icon" aria-hidden="true">
              <CircleCheck size={18} strokeWidth={1.75} />
            </span>
            <div class="min-w-0 flex-1">
              <p class="runtime-mobile-card-title">Nothing in progress</p>
              <p class="runtime-mobile-card-detail">
                When Medousa starts working, you’ll see it here.
              </p>
            </div>
          {/if}
        </div>
      </section>

      {#if runningCount > 0 || waitingCount > 0}
        <section class="runtime-mobile-section" aria-labelledby="runtime-motion-title">
          <h2 id="runtime-motion-title" class="runtime-mobile-section-title">In motion</h2>
          <div class="runtime-mobile-metrics">
            {#if runningCount > 0}
              <div class="runtime-mobile-metric">
                <span>Running</span>
                <strong>{runningCount}</strong>
              </div>
            {/if}
            {#if waitingCount > 0}
              <div class="runtime-mobile-metric">
                <span>Waiting</span>
                <strong>{waitingCount}</strong>
              </div>
            {/if}
          </div>
        </section>
      {/if}
    {:else if runtime.activeTab === "jobs"}
      <section class="runtime-mobile-section runtime-mobile-section-leading" aria-labelledby="runtime-jobs-active-title">
        <h2 id="runtime-jobs-active-title" class="runtime-mobile-section-title">Active</h2>
        <div class="runtime-mobile-card runtime-mobile-stack-card">
          {#if runtime.loading && !runtime.stats}
            <div class="runtime-mobile-empty-row">
              <span class="runtime-mobile-card-icon runtime-mobile-card-icon-active" aria-hidden="true">
                <Clock3 size={18} strokeWidth={1.75} />
              </span>
              <p class="runtime-mobile-card-title">Checking jobs…</p>
            </div>
          {:else if !runtime.stats}
            <div class="runtime-mobile-empty-row">
              <span class="runtime-mobile-card-icon runtime-mobile-card-icon-warning" aria-hidden="true">
                <CircleAlert size={18} strokeWidth={1.75} />
              </span>
              <div>
                <p class="runtime-mobile-card-title">Jobs are unavailable</p>
                <p class="runtime-mobile-card-detail">Try refreshing in a moment.</p>
              </div>
            </div>
          {:else if runningJobCount === 0 && waitingCount === 0}
            <div class="runtime-mobile-empty-row">
              <span class="runtime-mobile-card-icon" aria-hidden="true">
                <CircleCheck size={18} strokeWidth={1.75} />
              </span>
              <div>
                <p class="runtime-mobile-card-title">No jobs in progress</p>
                <p class="runtime-mobile-card-detail">The queue is clear.</p>
              </div>
            </div>
          {:else}
            {#if runningJobCount > 0}
              <div class="runtime-mobile-list-row">
                <span>Running</span><strong>{runningJobCount}</strong>
              </div>
            {/if}
            {#if waitingCount > 0}
              <div class="runtime-mobile-list-row">
                <span>Waiting</span><strong>{waitingCount}</strong>
              </div>
            {/if}
          {/if}
        </div>
      </section>

      {#if failedCount > 0 || needsReviewCount > 0}
        <section class="runtime-mobile-section" aria-labelledby="runtime-jobs-attention-title">
          <h2 id="runtime-jobs-attention-title" class="runtime-mobile-section-title runtime-mobile-section-title-warning">
            Needs attention
          </h2>
          <div class="runtime-mobile-card runtime-mobile-stack-card runtime-mobile-card-warning">
            {#if needsReviewCount > 0}
              <div class="runtime-mobile-list-row">
                <span>Needs review</span><strong>{needsReviewCount}</strong>
              </div>
            {/if}
            {#if failedCount > 0}
              <div class="runtime-mobile-list-row">
                <span>Stopped</span><strong>{failedCount}</strong>
              </div>
            {/if}
          </div>
        </section>
      {/if}

      {#if completedCount > 0}
        <section class="runtime-mobile-section" aria-labelledby="runtime-jobs-history-title">
          <h2 id="runtime-jobs-history-title" class="runtime-mobile-section-title">History</h2>
          <div class="runtime-mobile-card runtime-mobile-stack-card">
            <div class="runtime-mobile-list-row">
              <span>Completed</span><strong>{completedCount}</strong>
            </div>
          </div>
        </section>
      {/if}
    {:else if runtime.activeTab === "delivery"}
      <section class="runtime-mobile-section runtime-mobile-section-leading" aria-label="Delivery status">
        <div class="runtime-mobile-card runtime-mobile-current-card">
          <span
            class="runtime-mobile-card-icon"
            class:runtime-mobile-card-icon-active={runtime.loading && !runtime.delivery && !runtime.continuations}
            class:runtime-mobile-card-icon-warning={deliveryNeedsReviewCount > 0 || (runtime.error && !runtime.delivery && !runtime.continuations)}
            aria-hidden="true"
          >
            {#if runtime.error && !runtime.delivery && !runtime.continuations}
              <CircleAlert size={18} strokeWidth={1.75} />
            {:else if deliveryNeedsReviewCount > 0}
              <CircleAlert size={18} strokeWidth={1.75} />
            {:else if runtime.loading && !runtime.delivery && !runtime.continuations}
              <Clock3 size={18} strokeWidth={1.75} />
            {:else}
              <CircleCheck size={18} strokeWidth={1.75} />
            {/if}
          </span>
          <div class="min-w-0 flex-1">
            {#if runtime.loading && !runtime.delivery && !runtime.continuations}
              <p class="runtime-mobile-card-title">Checking delivery…</p>
            {:else if runtime.error && !runtime.delivery && !runtime.continuations}
              <p class="runtime-mobile-card-title">Delivery is unavailable</p>
              <p class="runtime-mobile-card-detail">Try refreshing in a moment.</p>
            {:else if deliveryNeedsReviewCount > 0}
              <p class="runtime-mobile-card-title">Some work needs review</p>
              <p class="runtime-mobile-card-detail">
                {countLabel(deliveryNeedsReviewCount, "follow-up")} couldn’t finish.
              </p>
            {:else if deliveryWaitingCount > 0 || followUpWaitingCount > 0}
              <p class="runtime-mobile-card-title">Delivery is in progress</p>
              <p class="runtime-mobile-card-detail">
                {countLabel(deliveryWaitingCount + followUpWaitingCount, "item")} waiting.
              </p>
            {:else}
              <p class="runtime-mobile-card-title">Delivery is ready</p>
              <p class="runtime-mobile-card-detail">Nothing is waiting to be sent.</p>
            {/if}
          </div>
        </div>
      </section>

      {#if runtime.delivery?.last_delivery_at_utc}
        <section class="runtime-mobile-section" aria-labelledby="runtime-last-delivery-title">
          <h2 id="runtime-last-delivery-title" class="runtime-mobile-section-title">Last delivery</h2>
          <div class="runtime-mobile-card runtime-mobile-stack-card">
            <div class="runtime-mobile-list-row">
              <span>Delivered</span>
              <strong>{formatWorkshopUpdatedAt(runtime.delivery.last_delivery_at_utc)}</strong>
            </div>
          </div>
        </section>
      {/if}

      {#if hasFollowUpHistory}
        <section class="runtime-mobile-section" aria-labelledby="runtime-followups-title">
          <h2 id="runtime-followups-title" class="runtime-mobile-section-title">Follow-ups</h2>
          <div class="runtime-mobile-card runtime-mobile-stack-card">
            {#if followUpWaitingCount > 0}
              <div class="runtime-mobile-list-row"><span>Waiting</span><strong>{followUpWaitingCount}</strong></div>
            {/if}
            {#if (runtime.continuations?.resumed_count ?? 0) > 0}
              <div class="runtime-mobile-list-row"><span>Resumed</span><strong>{runtime.continuations?.resumed_count}</strong></div>
            {/if}
            {#if (runtime.continuations?.consumed_count ?? 0) > 0}
              <div class="runtime-mobile-list-row"><span>Completed</span><strong>{runtime.continuations?.consumed_count}</strong></div>
            {/if}
          </div>
        </section>
      {/if}
    {/if}
  </main>

  <MobileRuntimeDock
    activeTab={runtime.activeTab}
    onTab={(tab) => (runtime.activeTab = tab)}
  />
</div>

<style>
  .runtime-mobile-shell {
    display: flex;
    height: 100%;
    min-height: 0;
    flex-direction: column;
    background: rgb(var(--color-surface-950));
  }

  .runtime-mobile-body {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 1.25rem 1.5rem 1.5rem;
  }

  .runtime-mobile-notice {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    margin-bottom: 1rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid rgb(var(--theme-warning) / 0.22);
    border-radius: 0.85rem;
    background: rgb(var(--theme-warning) / 0.07);
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.74rem;
  }

  .runtime-mobile-section + .runtime-mobile-section {
    margin-top: 1.35rem;
  }

  .runtime-mobile-section-leading {
    padding-top: 0.1rem;
  }

  .runtime-mobile-section-title {
    margin: 0 0 0.55rem 0.2rem;
    color: rgb(var(--theme-text-faint));
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .runtime-mobile-section-title-warning {
    color: rgb(var(--theme-warning) / 0.85);
  }

  .runtime-mobile-card {
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 1.15rem;
    background: rgb(var(--color-surface-900) / 0.58);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.035);
  }

  .runtime-mobile-current-card {
    display: flex;
    min-height: 5.25rem;
    align-items: flex-start;
    gap: 0.8rem;
    padding: 1rem;
  }

  .runtime-mobile-card-icon {
    display: inline-flex;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.8rem;
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--theme-success));
  }

  .runtime-mobile-card-icon-active {
    color: rgb(var(--theme-link));
  }

  .runtime-mobile-card-icon-warning {
    color: rgb(var(--theme-warning));
  }

  .runtime-mobile-card-title {
    color: rgb(var(--color-surface-100));
    font-size: 0.9rem;
    font-weight: 600;
    line-height: 1.35;
  }

  .runtime-mobile-card-detail {
    margin-top: 0.2rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.76rem;
    line-height: 1.4;
  }

  .runtime-mobile-card-meta {
    margin-top: 0.4rem;
    overflow: hidden;
    color: rgb(var(--theme-text-faint));
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.65rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .runtime-mobile-metrics {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(8rem, 1fr));
    gap: 0.65rem;
  }

  .runtime-mobile-metric {
    display: flex;
    min-height: 4.5rem;
    flex-direction: column;
    justify-content: space-between;
    padding: 0.85rem 0.9rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 1rem;
    background: rgb(var(--color-surface-900) / 0.48);
  }

  .runtime-mobile-metric span {
    color: rgb(var(--theme-text-faint));
    font-size: 0.7rem;
  }

  .runtime-mobile-metric strong {
    color: rgb(var(--color-surface-50));
    font-size: 1.25rem;
    font-weight: 600;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }

  .runtime-mobile-stack-card {
    overflow: hidden;
    padding: 0 0.9rem;
  }

  .runtime-mobile-card-warning {
    border-color: rgb(var(--theme-warning) / 0.22);
    background: rgb(var(--theme-warning) / 0.045);
  }

  .runtime-mobile-empty-row {
    display: flex;
    min-height: 5rem;
    align-items: center;
    gap: 0.8rem;
    padding: 0.75rem 0.1rem;
  }

  .runtime-mobile-list-row {
    display: flex;
    min-height: 3.4rem;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.82rem;
  }

  .runtime-mobile-list-row + .runtime-mobile-list-row {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.24);
  }

  .runtime-mobile-list-row strong {
    color: rgb(var(--color-surface-100));
    font-size: 0.82rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

</style>
