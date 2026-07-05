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

<section class="settings-subsection mt-8 border-t border-surface-500/35 pt-6">
  <header class="settings-section-header">
    <h3 class="text-sm font-semibold text-surface-50">Presentation cleanup</h3>
    <p class="workshop-faint mt-1 text-sm">
      Background job prunes stale HTML presentations and old revisions. Runs weekly via the
      runtime scheduler.
    </p>
  </header>

  {#if !isTauri()}
    <p class="workshop-faint mt-4 text-sm">Connect to the local engine to configure retention.</p>
  {:else if loading && !status}
    <p class="workshop-faint mt-4 text-sm">Loading retention settings…</p>
  {:else}
    <div class="mt-4 space-y-4">
      <label class="flex items-start gap-3">
        <input
          type="checkbox"
          class="mt-1"
          bind:checked={enabled}
          disabled={saving}
        />
        <span>
          <span class="block text-sm font-medium text-surface-100">Automatic cleanup</span>
          <span class="workshop-faint block text-xs leading-relaxed">
            When enabled, a recurring Stasis job runs maintenance on a weekly schedule.
          </span>
        </span>
      </label>

      <label class="block">
        <span class="block text-sm font-medium text-surface-100">Max age (days)</span>
        <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">
          Remove artifact index rows older than this. Default 90 days.
        </span>
        <input
          type="number"
          class="input mt-2 w-full max-w-xs"
          min="1"
          max="3650"
          bind:value={maxAgeDays}
          disabled={saving}
        />
      </label>

      <label class="block">
        <span class="block text-sm font-medium text-surface-100">Max per chat session</span>
        <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">
          Keep only the newest N artifact records per session; older ones are pruned.
        </span>
        <input
          type="number"
          class="input mt-2 w-full max-w-xs"
          min="1"
          max="10000"
          bind:value={maxPerSession}
          disabled={saving}
        />
      </label>

      {#if status}
        <dl class="workshop-faint grid gap-1 text-xs">
          <div class="flex gap-2">
            <dt class="text-surface-500">Next run</dt>
            <dd class="text-surface-300">{formatWhen(status.next_run_at_utc)}</dd>
          </div>
          <div class="flex gap-2">
            <dt class="text-surface-500">Last run</dt>
            <dd class="text-surface-300">{formatWhen(status.last_run_at_utc)}</dd>
          </div>
          {#if status.last_run_summary}
            <div class="flex gap-2">
              <dt class="shrink-0 text-surface-500">Result</dt>
              <dd class="truncate text-surface-400">{status.last_run_summary}</dd>
            </div>
          {/if}
        </dl>
      {/if}

      {#if error}
        <p class="text-sm text-red-400">{error}</p>
      {/if}
      {#if saved}
        <p class="text-sm text-emerald-400/90">Retention settings saved.</p>
      {/if}

      <button
        type="button"
        class="btn-primary mt-2"
        disabled={saving || !enabled && maxAgeDays < 1}
        onclick={() => void save()}
      >
        {saving ? "Saving…" : "Save presentation cleanup"}
      </button>
    </div>
  {/if}
</section>
