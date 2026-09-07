<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import {
    loadLatestWorldActivity,
    worldActivityDetailLabel,
    worldActivityTone,
    worldActivityWhen,
    worldActorLabel,
    worldEventLabel,
    type WorldActivityPage,
  } from "$lib/daemon";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import {
    Activity,
    ChevronRight,
    LoaderCircle,
    RefreshCw,
    ShieldCheck,
    X,
  } from "@lucide/svelte";

  type ActivityFilter = "all" | "evidence";

  let open = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let page = $state<WorldActivityPage | null>(null);
  let filter = $state<ActivityFilter>("all");
  let sheetEl = $state<HTMLElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);
  let requestGeneration = 0;

  const evidenceCount = $derived(page?.items.filter((item) => item.evidence).length ?? 0);
  const visibleItems = $derived(
    page?.items.filter((item) => filter === "all" || Boolean(item.evidence)) ?? [],
  );

  function show() {
    filter = "all";
    open = true;
  }

  function close() {
    requestGeneration += 1;
    open = false;
  }

  async function refresh(workshopId = workshops.activeWorkshopId) {
    const generation = ++requestGeneration;
    loading = true;
    error = null;
    try {
      const next = await loadLatestWorldActivity(null, 80);
      if (generation !== requestGeneration || workshopId !== workshops.activeWorkshopId) return;
      page = next;
    } catch (cause) {
      if (generation !== requestGeneration || workshopId !== workshops.activeWorkshopId) return;
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (generation === requestGeneration && workshopId === workshops.activeWorkshopId) {
        loading = false;
      }
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    close();
  }

  $effect(() => {
    const workshopId = workshops.activeWorkshopId;
    if (!open) return;
    page = null;
    void refresh(workshopId);
    return () => {
      requestGeneration += 1;
    };
  });

  $effect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      close();
      return true;
    });
  });

  $effect(() => {
    if (!open || !sheetEl || !headerEl || !window.matchMedia("(max-width: 767px)").matches) {
      return;
    }
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: close });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<section class="world-activity-band" aria-labelledby="world-activity-title">
  <div>
    <h3 id="world-activity-title" class="settings-subsection-heading">World activity</h3>
    <p class="settings-subsection-lead">
      Review browser and computer actions across people, agents, Bots, and workers.
    </p>
  </div>
  <button type="button" class="world-activity-launch" onclick={show}>
    <span class="world-activity-launch-icon" aria-hidden="true">
      <Activity size={16} strokeWidth={1.9} />
    </span>
    <span class="world-activity-launch-copy">
      <span>Review activity</span>
      <small>Authority, outcomes, recovery, and promoted evidence</small>
    </span>
    <ChevronRight size={16} strokeWidth={1.8} aria-hidden="true" />
  </button>
</section>

{#if open}
  <BodyPortal>
    <div
      class="world-activity-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        bind:this={sheetEl}
        class="world-activity-sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="world-activity-sheet-title"
      >
        <header bind:this={headerEl} class="world-activity-header">
          <div class="world-activity-grabber" aria-hidden="true"></div>
          <div class="world-activity-heading">
            <span class="world-activity-heading-icon" aria-hidden="true">
              <Activity size={17} strokeWidth={1.9} />
            </span>
            <div class="min-w-0 flex-1">
              <h2 id="world-activity-sheet-title">World activity</h2>
              <p>{workshops.activeLabel} · newest governed events</p>
            </div>
            <button
              type="button"
              class="world-activity-icon-button"
              aria-label="Refresh world activity"
              disabled={loading}
              onclick={() => void refresh()}
            >
              <RefreshCw
                size={16}
                strokeWidth={1.9}
                class={loading ? "animate-spin" : ""}
              />
            </button>
            <button
              type="button"
              class="world-activity-icon-button"
              aria-label="Close world activity"
              onclick={close}
            >
              <X size={18} strokeWidth={2} />
            </button>
          </div>
        </header>

        <div class="world-activity-toolbar" aria-label="World activity filters">
          <button type="button" class:active={filter === "all"} aria-pressed={filter === "all"} onclick={() => (filter = "all")}>
            All
            {#if page}<span>{page.items.length}</span>{/if}
          </button>
          <button type="button" class:active={filter === "evidence"} aria-pressed={filter === "evidence"} onclick={() => (filter = "evidence")}>
            Evidence
            {#if page}<span>{evidenceCount}</span>{/if}
          </button>
        </div>

        <div class="world-activity-body">
          {#if loading && !page}
            <div class="world-activity-state" aria-live="polite">
              <LoaderCircle size={20} strokeWidth={1.8} class="animate-spin" />
              Reading the workshop ledger…
            </div>
          {:else if error && !page}
            <div class="world-activity-error" role="status">
              <span>{error}</span>
              <button type="button" onclick={() => void refresh()}>Try again</button>
            </div>
          {:else if visibleItems.length === 0}
            <div class="world-activity-state">
              <Activity size={21} strokeWidth={1.6} />
              {filter === "evidence"
                ? "No sensitive or failed activity was promoted in this page."
                : "No governed browser or computer activity yet."}
            </div>
          {:else}
            <div class="world-activity-list">
              {#each visibleItems as item (item.event.sequence)}
                {@const event = item.event}
                {@const tone = worldActivityTone(event)}
                <details
                  class="world-activity-event"
                  class:world-activity-event-success={tone === "success"}
                  class:world-activity-event-warning={tone === "warning"}
                  class:world-activity-event-danger={tone === "danger"}
                >
                  <summary>
                    <span class="world-activity-event-marker" aria-hidden="true"></span>
                    <span class="world-activity-event-copy">
                      <span class="world-activity-event-title">
                        {worldEventLabel(event)}
                        {#if item.evidence}
                          <span class="world-activity-evidence-badge">
                            <ShieldCheck size={11} strokeWidth={2} /> Evidence
                          </span>
                        {/if}
                      </span>
                      <span class="world-activity-event-summary">
                        {event.summary?.trim() || `${worldActorLabel(event)} changed a ${event.surface} world`}
                      </span>
                      <span class="world-activity-event-meta">
                        {worldActorLabel(event)} · {worldActivityDetailLabel(event.surface)}
                        {#if event.status} · {worldActivityDetailLabel(event.status)}{/if}
                      </span>
                    </span>
                    <time datetime={new Date(event.recorded_at_ms).toISOString()}>
                      {worldActivityWhen(event.recorded_at_ms)}
                    </time>
                    <span class="world-activity-event-chevron" aria-hidden="true">
                      <ChevronRight size={15} strokeWidth={1.8} />
                    </span>
                  </summary>

                  <div class="world-activity-event-detail">
                    <dl>
                      <div>
                        <dt>Who</dt>
                        <dd>{worldActorLabel(event)}{event.principal_id ? ` · ${event.principal_id}` : ""}</dd>
                      </div>
                      <div>
                        <dt>Authority</dt>
                        <dd>{event.authority_id}</dd>
                      </div>
                      <div>
                        <dt>World</dt>
                        <dd>{event.world_id} · revision {event.world_revision}</dd>
                      </div>
                      <div>
                        <dt>Driver</dt>
                        <dd>{event.driver_id} · {worldActivityDetailLabel(event.ownership)}</dd>
                      </div>
                      {#if event.intent_id || event.trace_id}
                        <div>
                          <dt>Intent</dt>
                          <dd>{event.intent_id ?? "—"}{event.trace_id ? ` · ${event.trace_id}` : ""}</dd>
                        </div>
                      {/if}
                      {#if event.checkpoint}
                        <div>
                          <dt>Checkpoint</dt>
                          <dd>
                            Revision {event.checkpoint.world_revision}
                            {event.checkpoint.control_generation != null
                              ? ` · control ${event.checkpoint.control_generation}`
                              : ""}
                          </dd>
                        </div>
                      {/if}
                      {#if event.recovery}
                        <div>
                          <dt>Recovery</dt>
                          <dd>
                            {worldActivityDetailLabel(event.recovery.strategy)}
                            {event.recovery.requires_fresh_admission ? " · fresh admission required" : ""}
                          </dd>
                        </div>
                      {/if}
                      {#if item.evidence}
                        <div class="world-activity-evidence-detail">
                          <dt>Evidence</dt>
                          <dd>
                            {item.evidence.reasons.map(worldActivityDetailLabel).join(" · ")}
                            {item.evidence.action_elapsed_ms != null
                              ? ` · ${item.evidence.action_elapsed_ms} ms action`
                              : ""}
                            {item.evidence.durability_latency_us != null
                              ? ` · ${item.evidence.durability_latency_us} µs durability`
                              : ""}
                          </dd>
                        </div>
                      {/if}
                    </dl>
                  </div>
                </details>
              {/each}
            </div>
          {/if}

          {#if page?.hasEarlier}
            <p class="world-activity-footnote">Showing the newest 80 events. Older activity remains in the workshop ledger.</p>
          {/if}
          {#if page && !page.evidenceAvailable}
            <p class="world-activity-footnote">This workshop does not expose promoted evidence yet; its causal timeline is still available.</p>
          {/if}
          {#if error && page}
            <p class="world-activity-inline-error" role="status">Refresh failed: {error}</p>
          {/if}
        </div>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .world-activity-band {
    margin-top: 1.25rem;
  }

  .world-activity-launch {
    display: flex;
    width: 100%;
    margin-top: 0.7rem;
    align-items: center;
    gap: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900) / 0.22);
    padding: 0.7rem 0.75rem;
    text-align: left;
    color: rgb(var(--theme-text-primary));
  }

  .world-activity-launch:hover {
    border-color: rgb(var(--color-primary-400) / 0.34);
    background: rgb(var(--color-primary-500) / 0.07);
  }

  .world-activity-launch:focus-visible,
  .world-activity-icon-button:focus-visible,
  .world-activity-toolbar button:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.62);
    outline-offset: 2px;
  }

  .world-activity-launch-icon,
  .world-activity-heading-icon {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 0.58rem;
    background: rgb(var(--color-primary-500) / 0.11);
    color: rgb(var(--color-primary-300));
  }

  .world-activity-launch-icon {
    width: 2rem;
    height: 2rem;
  }

  .world-activity-launch-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.08rem;
    font-size: 0.75rem;
    font-weight: 590;
  }

  .world-activity-launch-copy small {
    overflow: hidden;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.66rem;
    font-weight: 400;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .world-activity-launch > :global(svg:last-child) {
    flex: none;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background: rgb(var(--color-surface-950) / 0.76);
    backdrop-filter: blur(8px);
  }

  .world-activity-sheet {
    display: flex;
    width: min(48rem, 100%);
    max-height: min(88dvh, 52rem);
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.38);
    border-radius: 1.2rem;
    background: rgb(var(--color-surface-900));
    color: rgb(var(--theme-text-primary));
    box-shadow: 0 26px 80px rgb(0 0 0 / 0.48);
  }

  .world-activity-header {
    flex: none;
    border-bottom: 1px solid rgb(var(--color-surface-600) / 0.28);
    padding: 0.9rem 1rem;
  }

  .world-activity-grabber {
    display: none;
  }

  .world-activity-heading {
    display: flex;
    align-items: center;
    gap: 0.7rem;
  }

  .world-activity-heading-icon {
    width: 2.15rem;
    height: 2.15rem;
  }

  .world-activity-heading h2,
  .world-activity-heading p,
  .world-activity-footnote,
  .world-activity-inline-error {
    margin: 0;
  }

  .world-activity-heading h2 {
    font-size: 0.92rem;
    font-weight: 650;
  }

  .world-activity-heading p {
    margin-top: 0.1rem;
    font-size: 0.68rem;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-icon-button {
    display: inline-flex;
    width: 2rem;
    height: 2rem;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 0.6rem;
    background: transparent;
    color: rgb(var(--theme-text-secondary));
  }

  .world-activity-icon-button:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.46);
    color: rgb(var(--theme-text-primary));
  }

  .world-activity-icon-button:disabled {
    opacity: 0.5;
  }

  .world-activity-toolbar {
    display: flex;
    flex: none;
    gap: 0.25rem;
    border-bottom: 1px solid rgb(var(--color-surface-600) / 0.22);
    padding: 0.55rem 1rem;
  }

  .world-activity-toolbar button {
    display: inline-flex;
    min-height: 1.85rem;
    align-items: center;
    gap: 0.35rem;
    border-radius: 999px;
    padding: 0.28rem 0.65rem;
    font-size: 0.68rem;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-toolbar button.active {
    background: rgb(var(--color-surface-700) / 0.62);
    color: rgb(var(--theme-text-primary));
  }

  .world-activity-toolbar button span {
    font-size: 0.61rem;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-body {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 0.75rem;
  }

  .world-activity-list {
    display: grid;
    gap: 0.42rem;
  }

  .world-activity-event {
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.28);
    border-radius: 0.72rem;
    background: rgb(var(--color-surface-950) / 0.22);
  }

  .world-activity-event summary {
    display: flex;
    min-height: 4.1rem;
    cursor: pointer;
    list-style: none;
    align-items: flex-start;
    gap: 0.6rem;
    padding: 0.68rem 0.72rem;
  }

  .world-activity-event summary::-webkit-details-marker {
    display: none;
  }

  .world-activity-event-marker {
    width: 0.47rem;
    height: 0.47rem;
    margin-top: 0.26rem;
    flex: none;
    border-radius: 999px;
    background: rgb(var(--theme-text-quiet));
    box-shadow: 0 0 0 3px rgb(var(--color-surface-700) / 0.42);
  }

  .world-activity-event-success .world-activity-event-marker {
    background: rgb(var(--color-success-400));
  }

  .world-activity-event-warning .world-activity-event-marker {
    background: rgb(var(--color-warning-400));
  }

  .world-activity-event-danger .world-activity-event-marker {
    background: rgb(var(--color-error-400));
  }

  .world-activity-event-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.12rem;
  }

  .world-activity-event-title {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.75rem;
    font-weight: 620;
  }

  .world-activity-event-summary,
  .world-activity-event-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .world-activity-event-summary {
    font-size: 0.69rem;
    color: rgb(var(--theme-text-secondary));
  }

  .world-activity-event-meta,
  .world-activity-event time {
    font-size: 0.61rem;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-event time {
    flex: none;
  }

  .world-activity-event-chevron {
    display: inline-flex;
    margin-top: 0.06rem;
    flex: none;
    color: rgb(var(--theme-text-quiet));
    transition: transform 120ms ease;
  }

  .world-activity-event[open] .world-activity-event-chevron {
    transform: rotate(90deg);
  }

  .world-activity-evidence-badge {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 0.2rem;
    border-radius: 999px;
    background: rgb(var(--color-primary-500) / 0.12);
    padding: 0.16rem 0.35rem;
    font-size: 0.56rem;
    font-weight: 600;
    color: rgb(var(--color-primary-300));
  }

  .world-activity-event-detail {
    border-top: 1px solid rgb(var(--color-surface-600) / 0.22);
    padding: 0.65rem 0.72rem 0.72rem 1.8rem;
  }

  .world-activity-event-detail dl {
    display: grid;
    margin: 0;
    gap: 0.45rem;
  }

  .world-activity-event-detail dl > div {
    display: grid;
    min-width: 0;
    grid-template-columns: 5rem minmax(0, 1fr);
    gap: 0.55rem;
  }

  .world-activity-event-detail dt {
    font-size: 0.59rem;
    font-weight: 650;
    letter-spacing: 0.045em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-event-detail dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.62rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-secondary));
  }

  .world-activity-event-detail .world-activity-evidence-detail dd {
    color: rgb(var(--color-primary-200));
  }

  .world-activity-state {
    display: flex;
    min-height: 12rem;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    text-align: center;
    font-size: 0.72rem;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-radius: 0.7rem;
    background: rgb(var(--color-error-500) / 0.1);
    padding: 0.65rem;
    font-size: 0.68rem;
    color: rgb(var(--color-error-300));
  }

  .world-activity-error button {
    flex: none;
    font-weight: 620;
    color: inherit;
  }

  .world-activity-footnote,
  .world-activity-inline-error {
    padding: 0.7rem 0.25rem 0.1rem;
    font-size: 0.61rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  .world-activity-inline-error {
    color: rgb(var(--color-error-300));
  }

  @media (max-width: 767px) {
    .world-activity-backdrop {
      align-items: flex-end;
      padding: 0;
    }

    .world-activity-sheet {
      width: 100%;
      max-height: 91dvh;
      border-right: 0;
      border-bottom: 0;
      border-left: 0;
      border-radius: 1.3rem 1.3rem 0 0;
    }

    .world-activity-header {
      padding-top: 0.45rem;
    }

    .world-activity-grabber {
      display: block;
      width: 2.6rem;
      height: 0.27rem;
      margin: 0 auto 0.45rem;
      border-radius: 999px;
      background: rgb(var(--color-surface-400) / 0.45);
    }

    .world-activity-body {
      padding: 0.6rem 0.65rem max(0.75rem, env(safe-area-inset-bottom));
    }

    .world-activity-event-detail {
      padding-left: 1.25rem;
    }

    .world-activity-event-detail dl > div {
      grid-template-columns: 4.35rem minmax(0, 1fr);
    }
  }
</style>
