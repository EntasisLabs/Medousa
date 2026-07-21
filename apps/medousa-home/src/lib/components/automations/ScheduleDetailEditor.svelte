<script lang="ts">
  import ScheduleEditorTitlebar from "$lib/components/automations/ScheduleEditorTitlebar.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import type { RecurringDefinitionEntry } from "$lib/types/recurring";
  import {
    cronToFriendly,
    describeFriendlySchedule,
  } from "$lib/utils/friendlySchedule";

  interface Props {
    entry: RecurringDefinitionEntry;
    hideSidebarExpand?: boolean;
    onDeleted?: () => void;
  }

  let {
    entry,
    hideSidebarExpand = false,
    onDeleted,
  }: Props = $props();

  const title = $derived(automations.labelFor(entry));
  const friendlyWhen = $derived(
    describeFriendlySchedule(cronToFriendly(entry.cron_expr), entry.timezone),
  );
  const lastFailed = $derived(entry.last_run_status === "failed");

  /** One story: paused owns the frame; failure only leads when active. */
  const heroLine = $derived.by(() => {
    if (!entry.enabled) {
      if (lastFailed) {
        return { text: "Paused · last run failed", tone: "muted" as const };
      }
      return { text: "Paused · won’t run until you resume", tone: "muted" as const };
    }
    if (lastFailed) {
      const when = entry.last_run_at_utc
        ? automations.formatNextRun(entry.last_run_at_utc)
        : "recently";
      return { text: `Last run failed · ${when}`, tone: "warn" as const };
    }
    return {
      text: `Active · next ${automations.formatNextRun(entry.next_run_at_utc)}`,
      tone: "ok" as const,
    };
  });

  const deliveryLine = $derived(
    `${automations.deliveryLabelFor(entry)} · ${
      entry.execution_mode === "prompt" ? "Quick prompt" : "Agent turn"
    }`,
  );
</script>

<div class="schedule-detail flex h-full min-h-0 flex-col">
  <ScheduleEditorTitlebar {entry} {hideSidebarExpand} {onDeleted} />

  <div class="schedule-detail-scroll min-h-0 flex-1 overflow-y-auto">
    <div class="schedule-detail-inner">
      <header class="schedule-detail-hero">
        <h1 class="schedule-detail-title">{title}</h1>
        <p
          class="schedule-detail-hero-line"
          class:schedule-detail-hero-line-warn={heroLine.tone === "warn"}
          class:schedule-detail-hero-line-ok={heroLine.tone === "ok"}
        >
          {heroLine.text}
        </p>
      </header>

      <section class="schedule-detail-when" aria-label="When it runs">
        <p class="schedule-detail-when-main">{friendlyWhen}</p>
        <p class="schedule-detail-delivery">{deliveryLine}</p>
      </section>

      {#if entry.prompt_excerpt?.trim()}
        <section class="schedule-detail-prompt" aria-label="What it does">
          <p class="schedule-detail-prompt-body">{entry.prompt_excerpt}</p>
        </section>
      {/if}

      <details class="schedule-detail-more">
        <summary>Details</summary>
        <dl class="schedule-detail-meta">
          <div>
            <dt>Origin</dt>
            <dd>{automations.originFor(entry)}</dd>
          </div>
          <div>
            <dt>Schedule</dt>
            <dd class="font-mono text-[12px]">
              {entry.cron_expr}
              <span class="text-surface-500"> · {entry.timezone}</span>
            </dd>
          </div>
          {#if entry.enabled}
            <div>
              <dt>Next run</dt>
              <dd>{automations.formatNextRun(entry.next_run_at_utc)}</dd>
            </div>
          {/if}
          <div>
            <dt>Last run</dt>
            <dd>
              {entry.last_run_at_utc
                ? automations.formatNextRun(entry.last_run_at_utc)
                : "—"}
              {#if entry.last_run_status}
                <span class:schedule-detail-fail={lastFailed}>
                  · {automations.statusLabel(entry.last_run_status)}
                </span>
              {/if}
            </dd>
          </div>
          <div class="schedule-detail-meta-wide">
            <dt>Id</dt>
            <dd class="font-mono text-[11px] text-surface-500">{entry.recurring_id}</dd>
          </div>
        </dl>
      </details>
    </div>
  </div>
</div>

<style>
  .schedule-detail-scroll {
    padding: 2rem 1.35rem 3rem;
  }

  @media (min-width: 640px) {
    .schedule-detail-scroll {
      padding: 2.75rem 2rem 3.5rem;
    }
  }

  .schedule-detail-inner {
    margin-inline: auto;
    width: 100%;
    max-width: 34rem;
  }

  .schedule-detail-hero {
    margin-bottom: 2rem;
  }

  .schedule-detail-title {
    margin: 0;
    font-size: clamp(1.65rem, 2.4vw, 2rem);
    font-weight: 580;
    letter-spacing: -0.035em;
    line-height: 1.15;
    color: rgb(var(--color-surface-50));
  }

  .schedule-detail-hero-line {
    margin: 0.65rem 0 0;
    font-size: 0.95rem;
    line-height: 1.4;
    letter-spacing: -0.015em;
    color: rgb(var(--color-surface-500));
  }

  .schedule-detail-hero-line-ok {
    color: rgb(var(--color-surface-350, var(--color-surface-300)));
  }

  .schedule-detail-hero-line-warn {
    color: rgb(var(--color-warning-400) / 0.95);
  }

  .schedule-detail-when {
    margin-bottom: 2rem;
  }

  .schedule-detail-when-main {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 520;
    letter-spacing: -0.025em;
    line-height: 1.3;
    color: rgb(var(--color-surface-100));
  }

  .schedule-detail-delivery {
    margin: 0.4rem 0 0;
    font-size: 0.8rem;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-500));
  }

  .schedule-detail-prompt {
    margin-bottom: 2.25rem;
    padding-top: 1.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.16);
  }

  .schedule-detail-prompt-body {
    margin: 0;
    white-space: pre-wrap;
    font-size: 1rem;
    font-weight: 420;
    line-height: 1.6;
    letter-spacing: -0.015em;
    color: rgb(var(--color-surface-200));
  }

  .schedule-detail-more {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.14);
    padding-top: 0.85rem;
  }

  .schedule-detail-more summary {
    cursor: pointer;
    list-style: none;
    font-size: 0.75rem;
    letter-spacing: 0.02em;
    color: rgb(var(--color-surface-500));
    user-select: none;
  }

  .schedule-detail-more summary::-webkit-details-marker {
    display: none;
  }

  .schedule-detail-more summary:hover {
    color: rgb(var(--color-surface-300));
  }

  .schedule-detail-more[open] summary {
    margin-bottom: 0.85rem;
    color: rgb(var(--color-surface-400));
  }

  .schedule-detail-meta {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.85rem 1.1rem;
    margin: 0;
  }

  .schedule-detail-meta-wide {
    grid-column: 1 / -1;
  }

  .schedule-detail-meta dt {
    margin: 0;
    font-size: 0.6rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-600));
  }

  .schedule-detail-meta dd {
    margin: 0.2rem 0 0;
    font-size: 0.78rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-400));
  }

  .schedule-detail-fail {
    color: rgb(var(--color-warning-400) / 0.9);
  }
</style>
