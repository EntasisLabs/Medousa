<script lang="ts">
  import {
    getArtifactRetentionStatus,
    updateArtifactRetention,
    type ArtifactRetentionStatus,
  } from "$lib/daemon";
  import { isTauri } from "$lib/window";
  import { onMount } from "svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let status = $state<ArtifactRetentionStatus | null>(null);
  let enabled = $state(true);
  let maxAgeDays = $state(90);
  let maxPerSession = $state(200);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  async function refresh() {
    if (!isTauri()) return;
    loading = true;
    error = null;
    try {
      status = await getArtifactRetentionStatus();
      enabled = status.settings.enabled;
      maxAgeDays = status.settings.max_age_days;
      maxPerSession = status.settings.max_per_session;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
  });

  async function save() {
    if (!isTauri() || saving) return;
    saving = true;
    error = null;
    saved = false;
    try {
      const response = await updateArtifactRetention({
        enabled,
        max_age_days: maxAgeDays,
        max_per_session: maxPerSession,
      });
      enabled = response.settings.enabled;
      maxAgeDays = response.settings.max_age_days;
      maxPerSession = response.settings.max_per_session;
      await refresh();
      saved = true;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function formatWhen(value: string | null | undefined): string {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }
</script>

<section class="settings-subsection mt-8">
  <h3 class="settings-subsection-heading">Presentations</h3>
  <p class="settings-subsection-lead">
    Old HTML decks and revisions clear on a weekly schedule.
  </p>

  {#if !isTauri()}
    <p class="workshop-faint text-sm">Connect to the local engine to configure cleanup.</p>
  {:else if loading && !status}
    <p class="workshop-faint text-sm">Loading cleanup settings…</p>
  {:else}
    <div class="settings-toggle-list">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Automatic cleanup</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Weekly background pass for stale presentations
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          bind:checked={enabled}
          disabled={saving}
        />
      </label>

      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Keep for</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Drop presentations older than this
          </span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input"
            min="1"
            max="3650"
            inputmode="numeric"
            bind:value={maxAgeDays}
            disabled={saving || !enabled}
            aria-label="Keep presentations for days"
          />
          <span class="settings-metric-unit" aria-hidden="true">days</span>
        </span>
      </label>

      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Per chat</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Newest kept; older ones in that session are pruned
          </span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input settings-metric-input-wide"
            min="1"
            max="10000"
            inputmode="numeric"
            bind:value={maxPerSession}
            disabled={saving || !enabled}
            aria-label="Max presentations per chat session"
          />
          <span class="settings-metric-unit" aria-hidden="true">max</span>
        </span>
      </label>
    </div>

    {#if status}
      <dl class="presentation-cleanup-meta">
        <div class="presentation-cleanup-meta-row">
          <dt>Next run</dt>
          <dd>{formatWhen(status.next_run_at_utc)}</dd>
        </div>
        <div class="presentation-cleanup-meta-row">
          <dt>Last run</dt>
          <dd>{formatWhen(status.last_run_at_utc)}</dd>
        </div>
        {#if status.last_run_summary}
          <div class="presentation-cleanup-meta-row">
            <dt>Result</dt>
            <dd class="truncate">{status.last_run_summary}</dd>
          </div>
        {/if}
      </dl>
    {/if}

    {#if error}
      <p class="mt-3 text-sm text-red-400">{error}</p>
    {/if}
    {#if saved}
      <p class="mt-3 text-sm text-emerald-400/90">Cleanup settings saved.</p>
    {/if}

    <div class="mt-4">
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={saving || (!enabled && maxAgeDays < 1)}
        onclick={() => void save()}
      >
        {saving ? "Saving…" : "Save cleanup"}
      </button>
    </div>
  {/if}
</section>

<style>
  .presentation-cleanup-meta {
    display: grid;
    gap: 0.25rem;
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .presentation-cleanup-meta-row {
    display: flex;
    gap: 0.5rem;
    min-width: 0;
  }

  .presentation-cleanup-meta-row dt {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    flex-shrink: 0;
  }

  .presentation-cleanup-meta-row dd {
    margin: 0;
    color: rgb(var(--shell-label, var(--color-surface-300)));
    min-width: 0;
  }
</style>
