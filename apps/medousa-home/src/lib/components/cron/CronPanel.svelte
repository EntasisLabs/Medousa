<script lang="ts">
  import { cronDraft } from "$lib/stores/cron.svelte";
  import { recurring } from "$lib/stores/recurring.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import type { RecurringDefinitionEntry } from "$lib/types/recurring";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  let search = $state("");
  let selectedId = $state<string | null>(null);

  const mobileDetailOpen = $derived(
    mobile && (selectedId !== null || cronDraft.showCreate),
  );
  let confirmDeleteId = $state<string | null>(null);

  let createPrompt = $state("");
  let createCron = $state("0 9 * * *");
  let createTimezone = $state("UTC");
  let createManuscript = $state<string | undefined>(undefined);

  const counts = $derived(recurring.activeCount());

  const filtered = $derived.by(() => {
    const query = search.trim().toLowerCase();
    const rows = [...recurring.definitions].sort(
      (left, right) =>
        new Date(left.next_run_at_utc).getTime() -
        new Date(right.next_run_at_utc).getTime(),
    );
    if (!query) return rows;
    return rows.filter((entry) => {
      const haystack = [
        recurring.labelFor(entry),
        entry.recurring_id,
        entry.cron_expr,
        entry.manuscript_id ?? "",
        recurring.originFor(entry),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  });

  const selected = $derived(
    selectedId
      ? (recurring.definitions.find((entry) => entry.recurring_id === selectedId) ??
        null)
      : null,
  );

  $effect(() => {
    if (!visible) return;
    void recurring.refresh();
  });

  $effect(() => {
    if (!visible || !cronDraft.showCreate || !cronDraft.createDraft) return;
    createPrompt = cronDraft.createDraft.prompt;
    createCron = cronDraft.createDraft.cron_expr;
    createTimezone = cronDraft.createDraft.timezone ?? "UTC";
    createManuscript = cronDraft.createDraft.manuscript_id;
    selectedId = null;
  });

  function openNew() {
    cronDraft.openCreate();
  }

  async function submitCreate() {
    await recurring.register({
      prompt: createPrompt.trim() || "Scheduled task",
      cron_expr: createCron.trim() || "0 9 * * *",
      manuscript_id: createManuscript,
      timezone: createTimezone.trim() || "UTC",
      model_hint: runtime.model,
      execution_mode: createManuscript ? "agent_turn" : "prompt",
    });
    cronDraft.clearCreate();
  }

  function selectEntry(entry: RecurringDefinitionEntry) {
    selectedId = entry.recurring_id;
    cronDraft.clearCreate();
    confirmDeleteId = null;
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 class="text-base font-semibold text-surface-50">Cron jobs</h1>
            <p class="text-xs text-surface-300">
              {counts.enabled}/{counts.total} active · sorted by next run
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            onclick={openNew}
          >
            + New cron
          </button>
        </div>
      {:else}
        <div class="flex items-center justify-between gap-2">
          <p class="workshop-faint text-xs">
            {counts.enabled}/{counts.total} active
          </p>
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            onclick={openNew}
          >
            + New
          </button>
        </div>
      {/if}

      <label class="mt-3 block">
        <span class="sr-only">Search cron jobs</span>
        <input
          class="input w-full text-sm"
          type="search"
          placeholder="Search cron jobs…"
          bind:value={search}
        />
      </label>
    </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div
      class="mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3 {mobileDetailOpen
        ? 'hidden'
        : ''}"
    >
      {#if recurring.loading && recurring.definitions.length === 0}
        <p class="workshop-muted">Loading schedules…</p>
      {:else if recurring.error}
        <p class="text-sm text-warning-400">{recurring.error}</p>
      {:else if filtered.length === 0}
        <p class="workshop-muted">
          {search.trim()
            ? "No cron jobs match your search."
            : "No cron jobs yet. Create one or schedule a skill."}
        </p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filtered as entry (entry.recurring_id)}
            <li>
              <button
                type="button"
                class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedId ===
                entry.recurring_id
                  ? 'bg-surface-800/80'
                  : ''}"
                onclick={() => selectEntry(entry)}
              >
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <p class="truncate font-medium text-surface-100">
                      {recurring.labelFor(entry)}
                    </p>
                    <span
                      class="text-[10px] uppercase tracking-wide {entry.enabled
                        ? 'text-primary-300'
                        : 'text-surface-500'}"
                    >
                      {entry.enabled ? "Scheduled" : "Paused"}
                    </span>
                    <span class="workshop-faint">{recurring.originFor(entry)}</span>
                  </div>
                  <p class="workshop-faint mt-0.5 truncate font-mono text-[11px]">
                    {entry.cron_expr} · {entry.timezone}
                  </p>
                  {#if entry.prompt_excerpt && entry.manuscript_id}
                    <p class="workshop-faint mt-1 truncate">{entry.prompt_excerpt}</p>
                  {/if}
                </div>
                <div class="shrink-0 text-right text-[11px] text-surface-400">
                  <p>Next {recurring.formatNextRun(entry.next_run_at_utc)}</p>
                  <p class="mt-0.5">
                    Last {entry.last_run_at_utc
                      ? recurring.formatNextRun(entry.last_run_at_utc)
                      : "—"}
                  </p>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if recurring.registerMessage}
        <p class="mt-4 text-xs text-primary-300">{recurring.registerMessage}</p>
      {/if}
    </div>

    <aside
      class="{mobile
        ? mobileDetailOpen
          ? 'mobile-you-scroll flex min-h-0 flex-1 flex-col overflow-y-auto'
          : 'hidden'
        : 'w-[min(360px,40%)] shrink-0 overflow-y-auto border-l border-surface-500/40 bg-surface-800/40'} px-4 py-4"
    >
      {#if mobileDetailOpen}
        <button
          type="button"
          class="workshop-text-action mb-3 shrink-0 text-sm"
          onclick={() => {
            selectedId = null;
            cronDraft.clearCreate();
            confirmDeleteId = null;
          }}
        >
          ← Back to list
        </button>
      {/if}
      {#if cronDraft.showCreate}
        <h2 class="workshop-section-title">New cron</h2>
        <form
          class="mt-3 space-y-3"
          onsubmit={(event) => {
            event.preventDefault();
            void submitCreate();
          }}
        >
          <label class="block">
            <span class="workshop-label">Prompt</span>
            <textarea
              class="textarea mt-1 w-full text-sm"
              rows="3"
              bind:value={createPrompt}
              placeholder="What should run on schedule?"
            ></textarea>
          </label>
          <label class="block">
            <span class="workshop-label">Cron</span>
            <input
              class="input mt-1 w-full font-mono text-[11px]"
              bind:value={createCron}
              placeholder="0 9 * * *"
            />
          </label>
          <label class="block">
            <span class="workshop-label">Timezone</span>
            <input
              class="input mt-1 w-full font-mono text-[11px]"
              bind:value={createTimezone}
              placeholder="UTC"
            />
          </label>
          {#if createManuscript}
            <p class="workshop-faint">
              Skill manuscript · <span class="font-mono">{createManuscript}</span>
            </p>
          {/if}
          <div class="flex gap-2 pt-1">
            <button
              type="submit"
              class="btn btn-sm variant-filled-primary"
              disabled={recurring.registering}
            >
              {recurring.registering ? "Saving…" : "Create"}
            </button>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() => cronDraft.clearCreate()}
            >
              Cancel
            </button>
          </div>
        </form>
      {:else if selected}
        <h2 class="workshop-section-title">Job detail</h2>
        <p class="mt-2 font-medium text-surface-100">{recurring.labelFor(selected)}</p>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{selected.recurring_id}</p>

        <dl class="mt-4 space-y-2 text-xs">
          <div>
            <dt class="workshop-label">Status</dt>
            <dd class="mt-0.5 text-surface-200">
              {selected.enabled ? "Scheduled" : "Paused"}
            </dd>
          </div>
          <div>
            <dt class="workshop-label">Origin</dt>
            <dd class="mt-0.5 text-surface-200">{recurring.originFor(selected)}</dd>
          </div>
          <div>
            <dt class="workshop-label">Cron</dt>
            <dd class="mt-0.5 font-mono text-surface-200">
              {selected.cron_expr} · {selected.timezone}
            </dd>
          </div>
          <div>
            <dt class="workshop-label">Next run</dt>
            <dd class="mt-0.5 text-surface-200">
              {recurring.formatNextRun(selected.next_run_at_utc)}
            </dd>
          </div>
          <div>
            <dt class="workshop-label">Last run</dt>
            <dd class="mt-0.5 text-surface-200">
              {selected.last_run_at_utc
                ? recurring.formatNextRun(selected.last_run_at_utc)
                : "—"}
            </dd>
          </div>
          {#if selected.prompt_excerpt}
            <div>
              <dt class="workshop-label">Prompt</dt>
              <dd class="mt-0.5 text-surface-300">{selected.prompt_excerpt}</dd>
            </div>
          {/if}
        </dl>

        <div class="mt-5 flex flex-wrap gap-2">
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            disabled={recurring.updatingId === selected.recurring_id}
            onclick={() =>
              void recurring.setEnabled(selected.recurring_id, !selected.enabled)}
          >
            {selected.enabled ? "Pause" : "Resume"}
          </button>
          {#if confirmDeleteId === selected.recurring_id}
            <button
              type="button"
              class="btn btn-sm variant-filled-error"
              disabled={recurring.deletingId === selected.recurring_id}
              onclick={async () => {
                await recurring.remove(selected.recurring_id);
                selectedId = null;
                confirmDeleteId = null;
              }}
            >
              Confirm delete
            </button>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() => (confirmDeleteId = null)}
            >
              Cancel
            </button>
          {:else}
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface text-error-400"
              onclick={() => (confirmDeleteId = selected.recurring_id)}
            >
              Delete…
            </button>
          {/if}
        </div>
      {:else}
        <p class="workshop-muted text-sm">
          Select a cron job to pause, resume, or delete — or create a new one.
        </p>
      {/if}
    </aside>
  </div>
</section>
