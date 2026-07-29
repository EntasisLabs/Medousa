<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import {
    beginHumanAttempt,
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
  } from "$lib/forge";
  import { terminalCreate } from "$lib/terminal";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { createAgentSession } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";
  import { setSessionAgentSessionId } from "$lib/utils/sessionAgentRuntime";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { vault } from "$lib/stores/vault.svelte";

  let title = $state("");
  let brief = $state("");
  let repoPath = $state("");
  let baseRef = $state("main");
  let patch = $state<EvidencePage | null>(null);
  let commands = $state<EvidencePage | null>(null);
  let worldInsight = $state<string>("");
  let worldFiles = $state<unknown>(null);
  let worldFind = $state<unknown>(null);
  let worldImpact = $state<unknown>(null);
  let worldBinding = $state<WorldBindingStatus | null>(null);
  let findQuery = $state("");
  let impactEntity = $state("");
  let busy = $state(false);
  let actionError = $state<string | null>(null);
  let worldMode = $state(false);
  let worldSnapshot = $state<"baseline" | "sealed">("sealed");

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
      let leaseId = undertakings.active?.leaseId ?? null;
      let generation = undertakings.active?.leaseGeneration ?? null;
      if (actions?.begin_attempt.allowed) {
        const begun = await beginHumanAttempt(d.id);
        leaseId = begun.lease.lease_id;
        generation = begun.lease.generation;
        undertakings.setActiveFromItem(begun.item, {
          leaseId,
          leaseGeneration: generation,
        });
      }
      const created = await terminalCreate({
        work_id: d.id,
        lease_id: leaseId,
      });
      const sid =
        typeof created.session_id === "string"
          ? created.session_id
          : String((created as { id?: string }).id ?? "");
      if (sid) {
        undertakings.bindTerminal(sid);
        shellTabs.openTerminal(sid, {
          activate: true,
          title: `Terminal · ${d.title}`,
        });
      }
      await undertakings.refreshDetail();
    });
  }

  async function startAgent(runtime: "codex" | "cursor") {
    const d = detail;
    if (!d || !chat.sessionId) return;
    await run(async () => {
      const accepted = await createAgentSession({
        session_id: chat.sessionId!,
        runtime,
        work_id: d.id,
      });
      setSessionAgentSessionId(chat.sessionId!, accepted.agent_session_id);
      undertakings.bindChat(chat.sessionId!);
      undertakings.setActiveFromItem(d);
      shellTabs.openChat(chat.sessionId!, { activate: true });
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
        const avec = await getWorldCodeAvec(detail!.id);
        worldInsight = JSON.stringify(avec, null, 2);
      } catch {
        worldInsight = "World not ready yet (indexing may still be running).";
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
      worldInsight = JSON.stringify(avec, null, 2);
    } catch (err) {
      worldInsight = err instanceof Error ? err.message : String(err);
    }
  }

  async function decideAndApply() {
    if (!review?.evidence_id || !review.evidence_digest || !detail) return;
    await run(async () => {
      const decided = await recordReviewIntent(detail.id, {
        evidence_id: review.evidence_id!,
        evidence_digest: review.evidence_digest!,
        strategy: "preserve_branch",
        rationale: "Approved from ForgeLens",
      });
      const decisionId = decided.review_decisions?.at(-1)?.id;
      if (decisionId) {
        await applyDecision(detail.id, decisionId);
      }
      await undertakings.refreshDetail();
    });
  }

  $effect(() => {
    if (review?.evidence_id) {
      void loadReviewExtras();
    }
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
    <button
      type="button"
      class="rounded-md border border-surface-500/50 px-2 py-1 text-xs"
      onclick={() => void undertakings.refreshList()}
    >
      Refresh
    </button>
  </header>

  {#if actionError || undertakings.error}
    <p class="rounded-md border border-amber-500/40 bg-amber-950/40 px-2 py-1 text-xs text-amber-100">
      {actionError || undertakings.error}
    </p>
  {/if}

  <div class="grid min-h-0 flex-1 gap-3 lg:grid-cols-[240px_1fr]">
    <aside class="flex min-h-0 flex-col gap-2 overflow-auto rounded-lg border border-surface-500/40 p-2">
      <form
        class="flex flex-col gap-1.5 border-b border-surface-500/30 pb-2"
        onsubmit={(e) => {
          e.preventDefault();
          void onCreate();
        }}
      >
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
          Create
        </button>
      </form>
      {#if undertakings.loading && undertakings.items.length === 0}
        <p class="text-xs text-surface-500">Loading…</p>
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

    <section class="flex min-h-0 flex-col gap-2 overflow-auto rounded-lg border border-surface-500/40 p-3">
      {#if !detail}
        <p class="text-sm text-surface-400">Select or create an undertaking.</p>
      {:else}
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div>
            <h3 class="text-lg font-semibold text-surface-50">{detail.title}</h3>
            <p class="text-xs text-surface-400">{detail.brief}</p>
            <p class="mt-1 font-mono text-[10px] text-surface-500" title={detail.state}>
              {humanPhaseLabel(detail.human_phase)}
              <span class="opacity-60">({detail.state})</span>
            </p>
          </div>
          <div class="flex flex-wrap gap-1">
            <button
              type="button"
              class="rounded border border-surface-500/50 px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy || !actions?.provision.allowed}
              title={actions?.provision.reason ?? ""}
              onclick={() => void undertakings.provision(detail.id)}
            >
              Provision
            </button>
            <button
              type="button"
              class="rounded border border-surface-500/50 px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy || !actions?.open_terminal.allowed}
              title={actions?.open_terminal.reason ?? ""}
              onclick={() => void openTerminalTracked()}
            >
              Work in Terminal
            </button>
            <button
              type="button"
              class="rounded border border-surface-500/50 px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy || !actions?.start_agent.allowed}
              title={actions?.start_agent.reason ?? ""}
              onclick={() => void startAgent("codex")}
            >
              Start Codex
            </button>
            <button
              type="button"
              class="rounded border border-surface-500/50 px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy || !actions?.start_agent.allowed}
              onclick={() => void startAgent("cursor")}
            >
              Start Cursor
            </button>
            <button
              type="button"
              class="rounded border border-surface-500/50 px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy || !actions?.seal.allowed}
              title={actions?.seal.reason ?? ""}
              onclick={() => void doSeal()}
            >
              Seal
            </button>
            <button
              type="button"
              class="rounded border border-rose-500/40 px-2 py-1 text-xs text-rose-200 disabled:opacity-40"
              disabled={busy || !actions?.discard.allowed}
              onclick={() =>
                void run(async () => {
                  await discardUndertaking(detail.id);
                  undertakings.clearActive();
                  await undertakings.refreshList();
                  undertakings.select("");
                })}
            >
              Discard
            </button>
            <button
              type="button"
              class="rounded border border-surface-500/50 px-2 py-1 text-xs"
              onclick={() => (worldMode = !worldMode)}
            >
              {worldMode ? "Hide World" : "World"}
            </button>
          </div>
        </div>

        {#if detail.environment}
          <p class="font-mono text-[11px] text-surface-400">
            worktree {detail.environment.worktree}
            · baseline {detail.environment.baseline_oid.slice(0, 10)}…
          </p>
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
                <li>{f.status} {f.path}</li>
              {/each}
            </ul>
            {#if patch}
              <pre
                class="mt-2 max-h-48 overflow-auto rounded bg-black/40 p-2 text-[10px] leading-snug text-surface-200"
              >{patch.lines.join("\n")}</pre>
            {/if}
            {#if commands && commands.lines.length}
              <p class="mt-2 text-[11px] font-medium text-surface-300">Commands</p>
              <pre
                class="mt-1 max-h-32 overflow-auto rounded bg-black/40 p-2 text-[10px] text-surface-300"
              >{commands.lines.join("\n")}</pre>
            {/if}
            {#if worldInsight}
              <div class="mt-2">
                <p class="text-[11px] font-medium text-surface-300">World insight (Code AVEC)</p>
                <pre
                  class="mt-1 max-h-40 overflow-auto rounded bg-black/40 p-2 text-[10px] text-surface-300"
                >{worldInsight}</pre>
              </div>
            {/if}
            {#if review.world}
              <p class="mt-1 text-[10px] text-surface-500">
                World baseline: {review.world.baseline?.state ?? "—"} · sealed:
                {review.world.sealed?.state ?? "—"}
              </p>
            {/if}
            <button
              type="button"
              class="mt-2 rounded bg-primary-500/80 px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy || !(actions?.apply.allowed || actions?.review.allowed)}
              onclick={() => void decideAndApply()}
            >
              Approve (Preserve Branch) & Apply
            </button>
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
                  onclick={() => (worldSnapshot = "baseline")}
                >
                  Baseline
                </button>
                <button
                  type="button"
                  class="rounded px-2 py-0.5 {worldSnapshot === 'sealed'
                    ? 'bg-surface-700 text-surface-50'
                    : 'text-surface-400'}"
                  onclick={() => (worldSnapshot = "sealed")}
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
                onclick={() =>
                  void run(async () => {
                    worldBinding = await getWorldBinding(detail.id);
                    worldFiles = await getWorldFiles(detail.id);
                    try {
                      worldInsight = JSON.stringify(
                        await getWorldCodeAvec(detail.id),
                        null,
                        2,
                      );
                    } catch (err) {
                      worldInsight =
                        err instanceof Error ? err.message : String(err);
                    }
                  })}
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
                <p class="text-[10px] text-surface-500">
                  Caps: {JSON.stringify(worldBinding.capabilities)}
                </p>
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
                    );
                  })}
              >
                Impact
              </button>
            </div>
            {#if worldFind}
              <pre
                class="mt-2 max-h-40 overflow-auto text-[10px] text-surface-300"
              >{JSON.stringify(worldFind, null, 2)}</pre>
            {/if}
            {#if worldImpact}
              <pre
                class="mt-2 max-h-40 overflow-auto text-[10px] text-surface-300"
              >{JSON.stringify(worldImpact, null, 2)}</pre>
            {/if}
            {#if worldInsight}
              <pre
                class="mt-2 max-h-40 overflow-auto text-[10px] text-surface-300"
              >{worldInsight}</pre>
            {/if}
            {#if worldFiles}
              <pre
                class="mt-2 max-h-48 overflow-auto text-[10px] text-surface-300"
              >{JSON.stringify(worldFiles, null, 2)}</pre>
            {/if}
          </div>
        {/if}
      {/if}
    </section>
  </div>
</div>
