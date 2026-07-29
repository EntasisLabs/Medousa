<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import {
    sealLease,
    recordReviewIntent,
    applyDecision,
    discardUndertaking,
    getEvidencePatch,
    getEvidenceCommands,
    getWorldCodeAvec,
    getWorldFiles,
    getWorldFind,
    getWorldImpact,
    getWorldBinding,
    queueWorldIndex,
    humanPhaseLabel,
    type EvidencePage,
    type WorldBindingStatus,
    type WorldAvecResult,
    type WorldFilesResult,
    type WorldFindResult,
    type WorldImpactResult,
    type WorldSnapshotRef,
  } from "$lib/forge";
  import {
    openTrackedTerminal,
    startTrackedAgent,
  } from "$lib/utils/undertakingWorkspace";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { vault } from "$lib/stores/vault.svelte";

  let title = $state("");
  let brief = $state("");
  let repoPath = $state("");
  let baseRef = $state("main");
  let patch = $state<EvidencePage | null>(null);
  let commands = $state<EvidencePage | null>(null);
  let worldInsight = $state<WorldAvecResult | null>(null);
  let worldFiles = $state<WorldFilesResult | null>(null);
  let worldFind = $state<WorldFindResult | null>(null);
  let worldImpact = $state<WorldImpactResult | null>(null);
  let worldError = $state<string | null>(null);
  let worldBinding = $state<WorldBindingStatus | null>(null);
  let findQuery = $state("");
  let impactEntity = $state("");
  let busy = $state(false);
  let actionError = $state<string | null>(null);
  let worldMode = $state(false);
  let worldSnapshot = $state<"baseline" | "sealed">("sealed");
  let creating = $state(false);
  let reviewRationale = $state("");
  let acknowledgePolicy = $state(false);

  const detail = $derived(undertakings.detail);
  const review = $derived(undertakings.review);
  const actions = $derived(detail?.allowed_actions);
  const activeItems = $derived(
    undertakings.items.filter(
      (i) => i.human_phase !== "complete" && i.state !== "discarded" && i.state !== "accepted",
    ),
  );
  const completedItems = $derived(
    undertakings.items.filter(
      (i) => i.human_phase === "complete" || i.state === "discarded" || i.state === "accepted",
    ),
  );

  function selectedWorldSnapshot(): WorldSnapshotRef | null {
    return worldBinding?.[worldSnapshot] ?? null;
  }

  onMount(() => {
    void undertakings.refreshList();
    undertakings.startPolling();
    // Default repo path only when Home shares the workshop disk.
    if (isCoLocatedWorkshop()) {
      const root = vault.activeVaultRoot;
      if (root?.path) repoPath = root.path;
    }
  });

  onDestroy(() => undertakings.stopPolling());

  async function onCreate() {
    if (!title.trim() || !repoPath.trim()) return;
    busy = true;
    actionError = null;
    try {
      await undertakings.create({
        title: title.trim(),
        brief: brief.trim() || title.trim(),
        repo_path: repoPath.trim(),
        base_ref: baseRef.trim() || "main",
      });
      title = "";
      brief = "";
      creating = false;
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function run(fn: () => Promise<void>) {
    busy = true;
    actionError = null;
    try {
      await fn();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
      if ((err as { status?: number }).status === 409 && undertakings.selectedId) {
        await undertakings.refreshDetail();
      }
    } finally {
      busy = false;
    }
  }

  async function openTerminalTracked() {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await openTrackedTerminal(d);
      await undertakings.refreshDetail();
    });
  }

  async function startAgent(runtime: "codex" | "cursor") {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await startTrackedAgent(d, runtime);
      await undertakings.refreshDetail();
    });
  }

  async function doSeal() {
    const leaseId = undertakings.active?.leaseId;
    const generation = undertakings.active?.leaseGeneration;
    if (!leaseId || generation == null) {
      actionError = "No active lease to seal";
      return;
    }
    await run(async () => {
      await sealLease(leaseId, generation);
      await undertakings.refreshDetail();
      if (undertakings.review?.evidence_id) {
        patch = await getEvidencePatch(undertakings.review.evidence_id, {
          work_id: undertakings.review.work_id,
          limit: 400,
        });
      }
      try {
        worldInsight = await getWorldCodeAvec(detail!.id);
        worldError = null;
      } catch (err) {
        worldError = err instanceof Error ? err.message : String(err);
      }
    });
  }

  async function loadReviewExtras() {
    if (!review?.evidence_id) return;
    patch = await getEvidencePatch(review.evidence_id, {
      work_id: review.work_id,
      limit: 400,
    });
    try {
      commands = await getEvidenceCommands(review.evidence_id, {
        work_id: review.work_id,
        limit: 100,
      });
    } catch {
      commands = null;
    }
    try {
      const avec = await getWorldCodeAvec(review.work_id);
      worldInsight = avec;
      worldError = null;
    } catch (err) {
      worldError = err instanceof Error ? err.message : String(err);
    }
  }

  async function loadMorePatch() {
    if (!patch?.truncated || !review?.evidence_id) return;
    await run(async () => {
      const next = await getEvidencePatch(review!.evidence_id!, {
        work_id: review!.work_id,
        offset: patch!.offset + patch!.lines.length,
        limit: 400,
      });
      patch = { ...next, offset: patch!.offset, lines: [...patch!.lines, ...next.lines] };
    });
  }

  async function loadMoreCommands() {
    if (!commands?.truncated || !review?.evidence_id) return;
    await run(async () => {
      const next = await getEvidenceCommands(review!.evidence_id!, {
        work_id: review!.work_id,
        offset: commands!.offset + commands!.lines.length,
        limit: 100,
      });
      commands = {
        ...next,
        offset: commands!.offset,
        lines: [...commands!.lines, ...next.lines],
      };
    });
  }

  async function loadWorldOverview() {
    if (!detail) return;
    await run(async () => {
      worldBinding = await getWorldBinding(detail!.id);
      const snapshot = selectedWorldSnapshot();
      worldFiles = await getWorldFiles(detail!.id, undefined, snapshot);
      worldInsight = await getWorldCodeAvec(detail!.id, snapshot);
      worldError = null;
    });
  }

  async function recordApproval() {
    if (!review?.evidence_id || !review.evidence_digest || !detail) return;
    await run(async () => {
      await recordReviewIntent(detail.id, {
        evidence_id: review.evidence_id!,
        evidence_digest: review.evidence_digest!,
        strategy: "preserve_branch",
        rationale: reviewRationale.trim() || "Reviewed in ForgeLens",
        acknowledged_violations: acknowledgePolicy
          ? (review.policy?.violations.map((violation) => violation.id) ?? [])
          : [],
      });
      await undertakings.refreshDetail();
    });
  }

  async function applyApproval() {
    if (!detail) return;
    const decisionId = review?.decision?.id ?? detail.review_decisions?.at(-1)?.id;
    if (!decisionId) return;
    if (!window.confirm("Apply this reviewed checkpoint and preserve its branch?")) return;
    await run(async () => {
      await applyDecision(detail!.id, decisionId);
      await undertakings.refreshDetail();
      await undertakings.refreshList();
    });
  }

  async function discardWithConfirmation() {
    if (!detail) return;
    if (!window.confirm(`Discard “${detail.title}”? The governed worktree will be released.`)) {
      return;
    }
    await run(async () => {
      await discardUndertaking(detail!.id);
      undertakings.clearActive();
      await undertakings.refreshList();
      await undertakings.select("");
    });
  }

  $effect(() => {
    if (review?.evidence_id) {
      void loadReviewExtras();
    }
  });

  $effect(() => {
    if (worldMode && detail?.id) void loadWorldOverview();
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-3 p-3 text-sm text-surface-100">
  <header class="flex flex-wrap items-center justify-between gap-2 border-b border-surface-500/40 pb-2">
    <div>
      <h2 class="text-base font-semibold text-surface-50">Undertakings</h2>
      <p class="text-xs text-surface-400">
        Governed work · custody, not activity cards
      </p>
    </div>
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        class="rounded-md border border-surface-500/50 px-2 py-1 text-xs text-surface-300"
        onclick={() => void undertakings.refreshList()}
      >
        Refresh
      </button>
      <button
        type="button"
        class="rounded-md bg-primary-500/80 px-2.5 py-1 text-xs font-medium text-surface-50"
        onclick={() => (creating = !creating)}
      >
        {creating ? "Cancel" : "New undertaking"}
      </button>
    </div>
  </header>

  {#if actionError || undertakings.error}
    <p class="rounded-md border border-amber-500/40 bg-amber-950/40 px-2 py-1 text-xs text-amber-100">
      {actionError || undertakings.error}
    </p>
  {/if}

  <div class="grid min-h-0 flex-1 gap-3 lg:grid-cols-[240px_1fr]">
    <aside class="flex min-h-0 flex-col gap-2 overflow-auto border-r border-surface-500/25 pr-3">
      {#if creating}
        <form
          class="flex flex-col gap-1.5 rounded-lg border border-surface-500/35 bg-surface-900/35 p-2"
          onsubmit={(e) => {
            e.preventDefault();
            void onCreate();
          }}
        >
          <p class="px-0.5 text-[10px] font-medium uppercase tracking-wide text-surface-400">
            New undertaking
          </p>
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder="Title"
          bind:value={title}
        />
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder="Brief"
          bind:value={brief}
        />
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder={isCoLocatedWorkshop()
            ? "Repo path (git root)"
            : "Repo path on workshop machine"}
          bind:value={repoPath}
        />
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder="Base ref"
          bind:value={baseRef}
        />
          <button
            type="submit"
            class="rounded bg-primary-500/80 px-2 py-1 text-xs font-medium text-surface-50 disabled:opacity-40"
            disabled={busy || !title.trim() || !repoPath.trim()}
          >
            Create undertaking
          </button>
        </form>
      {/if}
      {#if undertakings.loading && undertakings.items.length === 0}
        <p class="text-xs text-surface-500">Loading…</p>
      {:else if undertakings.items.length === 0 && !creating}
        <p class="px-1 py-3 text-xs leading-relaxed text-surface-500">
          No undertakings yet. Create one when a change deserves its own governed workspace.
        </p>
      {/if}
      <ul class="flex flex-col gap-1">
        {#if activeItems.length}
          <li class="px-1 pt-1 text-[10px] uppercase tracking-wide text-surface-500">
            Active
          </li>
          {#each activeItems as item (item.id)}
            <li>
              <button
                type="button"
                class="w-full rounded px-2 py-1.5 text-left text-xs hover:bg-surface-700/60 {undertakings.selectedId ===
                item.id
                  ? 'bg-surface-700/80'
                  : ''}"
                onclick={() => void undertakings.select(item.id)}
              >
                <span class="block truncate font-medium text-surface-50">{item.title}</span>
                <span class="text-[10px] text-surface-400">
                  {humanPhaseLabel(item.human_phase)} · {item.state}
                </span>
              </button>
            </li>
          {/each}
        {/if}
        {#if completedItems.length}
          <li class="px-1 pt-2 text-[10px] uppercase tracking-wide text-surface-500">
            Complete
          </li>
          {#each completedItems as item (item.id)}
            <li>
              <button
                type="button"
                class="w-full rounded px-2 py-1.5 text-left text-xs opacity-70 hover:bg-surface-700/60 {undertakings.selectedId ===
                item.id
                  ? 'bg-surface-700/80 opacity-100'
                  : ''}"
                onclick={() => void undertakings.select(item.id)}
              >
                <span class="block truncate font-medium text-surface-50">{item.title}</span>
                <span class="text-[10px] text-surface-400">
                  {humanPhaseLabel(item.human_phase)} · {item.state}
                </span>
              </button>
            </li>
          {/each}
        {/if}
      </ul>
    </aside>

    <section class="flex min-h-0 flex-col gap-2 overflow-auto px-1 py-2">
      {#if !detail}
        <div class="flex min-h-48 flex-1 items-center justify-center">
          <div class="max-w-sm text-center">
            <p class="text-sm font-medium text-surface-300">A place for intentional work</p>
            <p class="mt-1 text-xs leading-relaxed text-surface-500">
              Prepare a workspace, stay with it across Chat and Terminal, then preserve what
              matters through ForgeLens.
            </p>
          </div>
        </div>
      {:else}
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div>
            <h3 class="text-lg font-semibold text-surface-50">{detail.title}</h3>
            <p class="text-xs text-surface-400">{detail.brief}</p>
            <p class="mt-1 text-[11px] text-surface-400">
              {humanPhaseLabel(detail.human_phase)}
            </p>
          </div>
          <div class="flex items-center gap-1.5">
            {#if actions?.provision.allowed}
              <button
                type="button"
                class="rounded-md bg-primary-500/80 px-3 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void undertakings.provision(detail.id)}
              >
                Prepare workspace
              </button>
            {:else if actions?.seal.allowed}
              <button
                type="button"
                class="rounded-md bg-primary-500/80 px-3 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void doSeal()}
              >
                Review changes
              </button>
            {:else if actions?.open_terminal.allowed}
              <button
                type="button"
                class="rounded-md bg-primary-500/80 px-3 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void openTerminalTracked()}
              >
                Continue in Terminal
              </button>
            {/if}

            <details class="relative">
              <summary
                class="cursor-pointer list-none rounded-md border border-surface-500/45 px-2.5 py-1.5 text-xs text-surface-300 [&::-webkit-details-marker]:hidden"
              >
                More ···
              </summary>
              <div
                class="absolute right-0 top-full z-30 mt-1 w-48 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
              >
                <button
                  type="button"
                  class="secondary-action"
                  disabled={busy || !actions?.start_agent.allowed}
                  title={actions?.start_agent.reason ?? ""}
                  onclick={() => void startAgent("codex")}
                >Continue with Codex</button>
                <button
                  type="button"
                  class="secondary-action"
                  disabled={busy || !actions?.start_agent.allowed}
                  onclick={() => void startAgent("cursor")}
                >Continue with Cursor</button>
                <button
                  type="button"
                  class="secondary-action"
                  disabled={busy || !actions?.open_terminal.allowed}
                  onclick={() => void openTerminalTracked()}
                >Open another Terminal</button>
                <button
                  type="button"
                  class="secondary-action"
                  onclick={() => (worldMode = !worldMode)}
                >{worldMode ? "Hide World" : "Explore World"}</button>
                <div class="my-1 border-t border-surface-500/25"></div>
                <button
                  type="button"
                  class="secondary-action text-rose-200"
                  disabled={busy || !actions?.discard.allowed}
                  onclick={() => void discardWithConfirmation()}
                >Discard undertaking…</button>
              </div>
            </details>
          </div>
        </div>

        {#if detail.environment}
          <details class="text-[10px] text-surface-500">
            <summary class="w-fit cursor-pointer select-none hover:text-surface-300">
              Workspace details
            </summary>
            <p class="mt-1 break-all font-mono">
              {detail.environment.worktree}<br />baseline
              {detail.environment.baseline_oid.slice(0, 12)} · {detail.state}
            </p>
          </details>
        {/if}

        {#if review && (detail.human_phase === "review" || review.evidence_id)}
          <div class="mt-2 rounded-lg border border-primary-500/30 bg-surface-900/50 p-3">
            <h4 class="text-sm font-semibold text-surface-50">ForgeLens</h4>
            <p class="mt-1 text-[11px] text-surface-400">
              baseline {review.baseline_oid?.slice(0, 10)}… → sealed
              {review.sealed_head_oid?.slice(0, 10)}…
              {#if review.evidence_digest}
                · digest {review.evidence_digest.slice(0, 16)}…
              {/if}
              {#if review.truncated}
                · truncated
              {/if}
              {#if review.base_advanced}
                · base advanced
              {/if}
            </p>
            <ul class="mt-2 max-h-32 overflow-auto text-[11px] font-mono text-surface-300">
              {#each review.changed_files as f (f.path)}
                <li class="flex items-center gap-2 py-0.5">
                  <span class="w-14 shrink-0 text-surface-500">{f.status}</span>
                  <span class="min-w-0 flex-1 truncate">{f.path}</span>
                  {#if f.is_binary}
                    <span class="rounded bg-surface-700 px-1 py-0.5 text-[9px] text-surface-300">
                      binary{f.byte_size ? ` · ${Math.ceil(f.byte_size / 1024)} KB` : ""}
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
            {#if review.policy && (review.policy.violations.length || review.policy.capture_risks.length)}
              <div class="mt-2 rounded-md border border-amber-500/35 bg-amber-950/25 p-2">
                <p class="text-[11px] font-medium text-amber-100">Needs your attention</p>
                <ul class="mt-1 space-y-1 text-[10px] text-amber-100/80">
                  {#each review.policy.violations as violation (violation.id)}
                    <li><span class="font-mono">{violation.path}</span> — {violation.detail}</li>
                  {/each}
                  {#each review.policy.capture_risks as risk}
                    <li>
                      {#if risk.kind === "secret_pattern"}
                        Possible secret in <span class="font-mono">{risk.path}</span>
                      {:else if risk.kind === "oversize_file"}
                        Large file <span class="font-mono">{risk.path}</span>
                      {:else}
                        Checkpoint exceeds the configured size limit
                      {/if}
                    </li>
                  {/each}
                </ul>
                {#if review.policy.violations.length}
                  <label class="mt-2 flex items-start gap-2 text-[10px] text-amber-50">
                    <input type="checkbox" class="mt-0.5" bind:checked={acknowledgePolicy} />
                    I reviewed these policy exceptions and accept them for this checkpoint.
                  </label>
                {/if}
              </div>
            {/if}
            {#if patch}
              <pre
                class="mt-2 max-h-48 overflow-auto rounded bg-black/40 p-2 text-[10px] leading-snug text-surface-200"
              >{patch.lines.join("\n")}</pre>
              {#if patch.truncated}
                <button
                  type="button"
                  class="mt-1 text-[10px] text-primary-300 hover:underline disabled:opacity-40"
                  disabled={busy}
                  onclick={() => void loadMorePatch()}
                >Load more patch · {patch.lines.length} of {patch.total_lines} lines</button>
              {/if}
            {/if}
            {#if commands && commands.lines.length}
              <p class="mt-2 text-[11px] font-medium text-surface-300">Commands</p>
              <pre
                class="mt-1 max-h-32 overflow-auto rounded bg-black/40 p-2 text-[10px] text-surface-300"
              >{commands.lines.join("\n")}</pre>
              {#if commands.truncated}
                <button
                  type="button"
                  class="mt-1 text-[10px] text-primary-300 hover:underline disabled:opacity-40"
                  disabled={busy}
                  onclick={() => void loadMoreCommands()}
                >Load more commands · {commands.lines.length} of {commands.total_lines}</button>
              {/if}
            {/if}
            {#if worldInsight}
              <div class="mt-2 rounded-md bg-surface-950/35 p-2">
                <p class="text-[11px] font-medium text-surface-300">Code understanding</p>
                {#if worldInsight.code_avec}
                  <p class="mt-1 text-[10px] text-surface-400">
                    {worldInsight.code_avec.fully_scored_entities} of
                    {worldInsight.code_avec.scoreable_entities} scoreable entities have complete
                    analysis.
                  </p>
                {/if}
              </div>
            {/if}
            {#if review.world}
              <p class="mt-1 text-[10px] text-surface-500">
                World baseline: {review.world.baseline?.state ?? "—"} · sealed:
                {review.world.sealed?.state ?? "—"}
              </p>
            {/if}
            {#if actions?.review.allowed}
              <label class="mt-3 block text-[10px] text-surface-400" for="review-rationale">
                Review note <span class="text-surface-600">(optional)</span>
              </label>
              <textarea
                id="review-rationale"
                rows="2"
                class="mt-1 w-full resize-none rounded-md border border-surface-500/40 bg-surface-950/50 px-2 py-1.5 text-xs text-surface-100 placeholder:text-surface-600"
                placeholder="What made this checkpoint ready?"
                bind:value={reviewRationale}
              ></textarea>
              <button
                type="button"
                class="mt-2 rounded bg-primary-500/80 px-2.5 py-1.5 text-xs font-medium disabled:opacity-40"
                disabled={busy || (!!review.policy?.violations.length && !acknowledgePolicy)}
                onclick={() => void recordApproval()}
              >
                Approve checkpoint
              </button>
            {:else if actions?.apply.allowed}
              <div class="mt-3 flex items-center justify-between gap-3 rounded-md border border-primary-500/25 bg-primary-950/15 p-2">
                <p class="text-[11px] text-surface-300">
                  Approved. The checkpoint is ready to be preserved.
                </p>
                <button
                  type="button"
                  class="shrink-0 rounded bg-primary-500/80 px-2.5 py-1.5 text-xs font-medium disabled:opacity-40"
                  disabled={busy}
                  onclick={() => void applyApproval()}
                >
                  Apply…
                </button>
              </div>
            {/if}
          </div>
        {/if}

        {#if worldMode}
          <div class="rounded-lg border border-surface-500/40 p-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h4 class="text-sm font-semibold">World explorer</h4>
              <div class="flex gap-1 text-[10px]">
                <button
                  type="button"
                  class="rounded px-2 py-0.5 {worldSnapshot === 'baseline'
                    ? 'bg-surface-700 text-surface-50'
                    : 'text-surface-400'}"
                  onclick={() => {
                    worldSnapshot = "baseline";
                    void loadWorldOverview();
                  }}
                >
                  Baseline
                </button>
                <button
                  type="button"
                  class="rounded px-2 py-0.5 {worldSnapshot === 'sealed'
                    ? 'bg-surface-700 text-surface-50'
                    : 'text-surface-400'}"
                  onclick={() => {
                    worldSnapshot = "sealed";
                    void loadWorldOverview();
                  }}
                >
                  Sealed
                </button>
              </div>
            </div>
            <p class="mt-1 text-[10px] text-surface-500">
              Observe-only · snapshot preference: {worldSnapshot}. Mutate via Forge
              actions above.
            </p>
            <div class="mt-2 flex flex-wrap gap-1">
              <button
                type="button"
                class="rounded border border-surface-500/50 px-2 py-1 text-xs"
                onclick={() => void loadWorldOverview()}
              >
                Refresh coverage
              </button>
              <button
                type="button"
                class="rounded border border-surface-500/50 px-2 py-1 text-xs"
                onclick={() =>
                  void run(async () => {
                    await queueWorldIndex(detail.id, worldSnapshot);
                    worldBinding = await getWorldBinding(detail.id);
                  })}
              >
                Reindex {worldSnapshot}
              </button>
            </div>
            {#if worldBinding}
              <p class="mt-2 text-[10px] text-surface-400">
                Binding baseline: {worldBinding.baseline?.state ?? "—"} · sealed:
                {worldBinding.sealed?.state ?? "—"}
              </p>
              {#if worldBinding.capabilities}
                <div class="mt-1 flex flex-wrap gap-1">
                  {#each Object.entries(worldBinding.capabilities).filter(([key]) => key !== "note") as [capability, enabled]}
                    <span
                      class="rounded-full border border-surface-500/30 px-1.5 py-0.5 text-[9px] {enabled
                        ? 'text-surface-300'
                        : 'text-surface-600'}"
                    >{capability.replaceAll("_", " ")}{enabled ? "" : " · unavailable"}</span>
                  {/each}
                </div>
              {/if}
              {#if worldBinding.diagnostics?.length}
                <ul class="mt-1 text-[10px] text-amber-200/90">
                  {#each worldBinding.diagnostics as d}
                    <li>{d}</li>
                  {/each}
                </ul>
              {/if}
            {/if}
            <div class="mt-2 flex flex-wrap items-center gap-1">
              <input
                class="min-w-[120px] flex-1 rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
                placeholder="Find name contains…"
                bind:value={findQuery}
              />
              <button
                type="button"
                class="rounded border border-surface-500/50 px-2 py-1 text-xs"
                onclick={() =>
                  void run(async () => {
                    worldFind = await getWorldFind(detail.id, {
                      name_contains: findQuery.trim() || undefined,
                      snapshot: selectedWorldSnapshot(),
                    });
                  })}
              >
                Find
              </button>
            </div>
            <div class="mt-1 flex flex-wrap items-center gap-1">
              <input
                class="min-w-[120px] flex-1 rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
                placeholder="Entity id for impact"
                bind:value={impactEntity}
              />
              <button
                type="button"
                class="rounded border border-surface-500/50 px-2 py-1 text-xs"
                disabled={!impactEntity.trim()}
                onclick={() =>
                  void run(async () => {
                    worldImpact = await getWorldImpact(
                      detail.id,
                      impactEntity.trim(),
                      selectedWorldSnapshot(),
                    );
                  })}
              >
                Impact
              </button>
            </div>
            {#if worldFind}
              <div class="mt-2 max-h-44 overflow-auto rounded-md border border-surface-500/25">
                {#if worldFind.entities.length === 0}
                  <p class="p-2 text-[10px] text-surface-500">No matching entities.</p>
                {:else}
                  {#each worldFind.entities as entity (entity.id)}
                    <button
                      type="button"
                      class="flex w-full items-center justify-between gap-2 border-b border-surface-500/20 px-2 py-1.5 text-left last:border-0 hover:bg-surface-800/60"
                      onclick={() => {
                        impactEntity = entity.id;
                        undertakings.setSelection({ entityId: entity.id, path: entity.path });
                      }}
                    >
                      <span class="min-w-0">
                        <span class="block truncate text-[11px] text-surface-200">{entity.label}</span>
                        <span class="block truncate font-mono text-[9px] text-surface-500">{entity.path}</span>
                      </span>
                      <span class="shrink-0 text-[9px] text-surface-500">{entity.kind}</span>
                    </button>
                  {/each}
                {/if}
              </div>
            {/if}
            {#if worldImpact}
              <div class="mt-2 rounded-md border border-surface-500/25 p-2">
                <p class="text-[11px] font-medium text-surface-200">
                  Impact · {worldImpact.direct_dependents ?? 0} direct,
                  {worldImpact.transitive_dependents ?? 0} transitive
                </p>
                {#if worldImpact.message}
                  <p class="mt-1 text-[10px] text-surface-500">{worldImpact.message}</p>
                {/if}
                <ul class="mt-1 max-h-32 overflow-auto text-[10px] text-surface-400">
                  {#each worldImpact.nodes as node (node.id)}
                    <li class="truncate py-0.5">{node.label} <span class="text-surface-600">· {node.path}</span></li>
                  {/each}
                </ul>
              </div>
            {/if}
            {#if worldInsight}
              <div class="mt-2 grid gap-2 sm:grid-cols-3">
                <div class="rounded-md bg-surface-900/60 p-2">
                  <p class="text-lg font-semibold text-surface-100">
                    {worldInsight.code_avec?.fully_scored_entities ?? 0}
                  </p>
                  <p class="text-[9px] text-surface-500">fully analyzed</p>
                </div>
                <div class="rounded-md bg-surface-900/60 p-2">
                  <p class="text-lg font-semibold text-surface-100">
                    {worldInsight.code_avec?.scoreable_entities ?? 0}
                  </p>
                  <p class="text-[9px] text-surface-500">scoreable entities</p>
                </div>
                <div class="rounded-md bg-surface-900/60 p-2">
                  <p class="text-lg font-semibold text-surface-100">
                    {worldInsight.code_avec?.gaps.length ?? 0}
                  </p>
                  <p class="text-[9px] text-surface-500">coverage gaps</p>
                </div>
              </div>
            {/if}
            {#if worldFiles}
              <details class="mt-2">
                <summary class="cursor-pointer text-[10px] text-surface-400">
                  Files in this snapshot · {worldFiles.files.length}
                </summary>
                <ul class="mt-1 max-h-48 overflow-auto rounded-md border border-surface-500/25">
                  {#each worldFiles.files as file (file.id)}
                    <li class="border-b border-surface-500/15 px-2 py-1 last:border-0">
                      <button
                        type="button"
                        class="w-full truncate text-left font-mono text-[10px] text-surface-400 hover:text-surface-100"
                        onclick={() => undertakings.setSelection({ entityId: file.id, path: file.path })}
                      >{file.path}</button>
                    </li>
                  {/each}
                </ul>
              </details>
            {/if}
            {#if worldError}
              <p class="mt-2 rounded-md bg-amber-950/30 p-2 text-[10px] text-amber-100">
                World is not ready yet. {worldError}
              </p>
            {/if}
          </div>
        {/if}
      {/if}
    </section>
  </div>
</div>

<style>
  .secondary-action {
    display: block;
    width: 100%;
    border-radius: 0.4rem;
    padding: 0.4rem 0.5rem;
    text-align: left;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-200));
  }

  .secondary-action:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.65);
    color: rgb(var(--color-surface-50));
  }

  .secondary-action:disabled {
    opacity: 0.35;
  }
</style>
