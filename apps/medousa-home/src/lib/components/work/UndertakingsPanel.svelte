<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import {
    sealLease,
    prepareExecutorHandoff,
    recordReviewIntent,
    applyDecision,
    discardUndertaking,
    getEvidencePatch,
    getEvidenceCommands,
    restoreReviewFile,
    getWorldCodeAvec,
    getWorldFiles,
    getWorldFind,
    getWorldImpact,
    getWorldAtLocation,
    getWorldBinding,
    queueWorldIndex,
    exportUndertakingBundle,
    humanPhaseGuidance,
    humanPhaseLabel,
    humanizeForgeMessage,
    getProviderHandoff,
    shareProviderHandoff,
    saveProviderContext,
    getProviderComments,
    importProviderComment,
    type EvidencePage,
    type ReviewFileDiff,
    type WorldBindingStatus,
    type WorldAvecResult,
    type WorldFilesResult,
    type WorldFindResult,
    type WorldImpactResult,
    type WorldSnapshotRef,
    type ProviderHandoff,
    type ProviderComment,
  } from "$lib/forge";
  import {
    openTrackedTerminal,
    reclaimTrackedHuman,
    startTrackedAgent,
  } from "$lib/utils/undertakingWorkspace";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { vault } from "$lib/stores/vault.svelte";
  import { openUndertakingLocation } from "$lib/utils/undertakingLocation";
  import { undertakingLocationDeepLinkUrl } from "$lib/deepLinks";
  import { shareText } from "$lib/share";
  import CodeSourceEditor from "$lib/components/work/CodeSourceEditor.svelte";
  import ForgeReviewSurface from "$lib/components/work/ForgeReviewSurface.svelte";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";

  interface Props {
    /** The Workspace Code explorer owns creation and undertaking selection. */
    showBrowser?: boolean;
  }

  let { showBrowser = true }: Props = $props();

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
  let exportOpen = $state(false);
  let exportDestination = $state("");
  let exportedDestination = $state<string | null>(null);
  let reviewEl = $state<HTMLDivElement | null>(null);
  let worldEl = $state<HTMLDivElement | null>(null);
  let preferredCodeAgent = $state<"codex" | "cursor">("codex");
  let providerHandoff = $state<ProviderHandoff | null>(null);
  let providerComments = $state<ProviderComment[]>([]);
  let providerLink = $state("");
  let providerOpen = $state(false);

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
    const savedAgent = localStorage.getItem("medousa-code-agent-runtime");
    if (savedAgent === "cursor" || savedAgent === "codex") preferredCodeAgent = savedAgent;
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

  async function toggleWorldFromEditor() {
    worldMode = !worldMode;
    if (!worldMode) return;
    await tick();
    worldEl?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  async function openReviewFromEditor() {
    await tick();
    reviewEl?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  async function startAgent(runtime: "codex" | "cursor") {
    const d = detail;
    if (!d) return;
    preferredCodeAgent = runtime;
    localStorage.setItem("medousa-code-agent-runtime", runtime);
    await run(async () => {
      await startTrackedAgent(d, runtime);
      await undertakings.refreshDetail();
    });
  }

  async function handoffToAgent(runtime: "codex" | "cursor", draft?: string) {
    const d = detail;
    if (!d) return;
    preferredCodeAgent = runtime;
    localStorage.setItem("medousa-code-agent-runtime", runtime);
    await run(async () => {
      const active = undertakings.active;
      let ready = d;
      if (
        active?.workId === d.id &&
        active.leaseId &&
        active.leaseGeneration != null
      ) {
        ready = await prepareExecutorHandoff({
          work_id: d.id,
          lease_id: active.leaseId,
          generation: active.leaseGeneration,
          to_executor: runtime,
        });
        undertakings.setActiveFromItem(ready);
      }
      await startTrackedAgent(ready, runtime, { draft });
      await undertakings.refreshDetail();
    });
  }

  async function reclaimHuman() {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await reclaimTrackedHuman(d);
      await undertakings.refreshDetail();
    });
  }

  async function doSeal() {
    const leaseId = undertakings.active?.leaseId;
    const generation = undertakings.active?.leaseGeneration;
    if (!leaseId || generation == null) {
      actionError = "Editing is not active yet. Open a file or ask an agent to begin.";
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

  async function revealLocation(input: {
    path: string;
    line?: number | null;
    entityId?: string | null;
  }) {
    if (!detail) return;
    await openUndertakingLocation({ workId: detail.id, ...input });
    worldMode = true;
    impactEntity = input.entityId ?? "";
    if (input.entityId) {
      worldImpact = await getWorldImpact(
        detail.id,
        input.entityId,
        selectedWorldSnapshot(),
      );
    } else if (input.line) {
      const located = await getWorldAtLocation(
        detail.id,
        input.path,
        input.line,
        selectedWorldSnapshot(),
      );
      const entity = located.entity;
      if (entity) {
        impactEntity = entity.id;
        undertakings.setSelection({ entityId: entity.id });
      }
    }
  }

  async function copyLocationLink() {
    const active = undertakings.active;
    if (!active?.selectedPath) return;
    const url = undertakingLocationDeepLinkUrl({
      workId: active.workId,
      path: active.selectedPath,
      line: active.selectedLine,
      entityId: active.selectedEntityId,
    });
    const result = await shareText("Project location", url);
    if (result === "failed") actionError = "Could not copy this location";
  }

  function exportFolderName(value: string): string {
    const slug = value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 60);
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    return `${slug || "project"}-medousa-record-${stamp}`;
  }

  async function beginExport() {
    if (!detail) return;
    exportedDestination = null;
    if (isCoLocatedWorkshop()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const parent = await open({
        directory: true,
        multiple: false,
        canCreateDirectories: true,
        title: "Choose where to save this project record",
      });
      if (typeof parent !== "string") return;
      exportDestination = `${parent.replace(/[\\/]$/, "")}/${exportFolderName(detail.title)}`;
    } else {
      exportDestination = "";
    }
    exportOpen = true;
  }

  async function confirmExport() {
    if (!detail || !exportDestination.trim()) return;
    await run(async () => {
      const result = await exportUndertakingBundle(detail!.id, exportDestination.trim());
      exportedDestination = result.destination;
      exportOpen = false;
    });
  }

  async function recordApproval() {
    if (!review?.evidence_id || !review.evidence_digest || !detail) return;
    await run(async () => {
      await recordReviewIntent(detail.id, {
        evidence_id: review.evidence_id!,
        evidence_digest: review.evidence_digest!,
        strategy: "preserve_branch",
        rationale: reviewRationale.trim() || "Reviewed in Medousa",
        acknowledged_violations: acknowledgePolicy
          ? (review.policy?.violations.map((violation) => violation.id) ?? [])
          : [],
      });
      await undertakings.refreshDetail();
    });
  }

  async function restoreReviewedFile(comparison: ReviewFileDiff) {
    if (!detail) return;
    await run(async () => {
      const result = await restoreReviewFile(detail!.id, {
        path: comparison.path,
        expected_reviewed_oid: comparison.reviewed_oid,
      });
      undertakings.setActiveFromItem(result.item);
      await undertakings.refreshDetail();
      await codeWorkspace.open(detail!.id, result.path, 1);
      undertakings.setSelection({ path: result.path, line: 1, entityId: null });
    });
  }

  async function applyApproval() {
    if (!detail) return;
    const decisionId = review?.decision?.id ?? detail.review_decisions?.at(-1)?.id;
    if (!decisionId) return;
    if (!window.confirm("Finish this project and keep its branch?")) return;
    await run(async () => {
      await applyDecision(detail!.id, decisionId);
      await undertakings.refreshDetail();
      await undertakings.refreshList();
    });
  }

  async function loadProviderHandoff() {
    if (!detail) return;
    try {
      providerHandoff = await getProviderHandoff(detail.id);
    } catch {
      providerHandoff = null;
    }
  }

  async function shareProject() {
    if (!detail) return;
    await run(async () => {
      providerHandoff = await shareProviderHandoff(detail!.id, {
        title: detail!.title,
      });
      if (providerHandoff.review_url) {
        window.open(providerHandoff.review_url, "_blank", "noopener,noreferrer");
      }
    });
  }

  async function addProviderLink() {
    if (!detail || !providerHandoff || !providerLink.trim()) return;
    await run(async () => {
      providerHandoff = await saveProviderContext(detail!.id, [
        ...providerHandoff!.links,
        providerLink.trim(),
      ]);
      providerLink = "";
    });
  }

  async function loadProviderComments() {
    if (!detail) return;
    providerOpen = !providerOpen;
    if (!providerOpen) return;
    await run(async () => {
      providerComments = await getProviderComments(detail!.id);
    });
  }

  async function createFollowUp(comment: ProviderComment) {
    if (!detail) return;
    await run(async () => {
      const item = await importProviderComment(detail!.id, comment);
      await undertakings.refreshList();
      await undertakings.select(item.id);
    });
  }

  async function discardWithConfirmation() {
    if (!detail) return;
    if (!window.confirm(`Discard “${detail.title}”? Its working copy will be removed.`)) {
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
      void loadProviderHandoff();
    }
  });

  $effect(() => {
    if (worldMode && detail?.id) void loadWorldOverview();
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-3 p-3 text-sm text-surface-100">
  {#if showBrowser}<header class="flex flex-wrap items-center justify-between gap-2 border-b border-surface-500/40 pb-2">
    <div>
      <h2 class="text-base font-semibold text-surface-50">Code projects</h2>
      <p class="text-xs text-surface-400">
        One goal, its files, and everyone helping
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
        {creating ? "Cancel" : "New project"}
      </button>
    </div>
  </header>{/if}

  {#if actionError || undertakings.error}
    <p class="rounded-md border border-amber-500/40 bg-amber-950/40 px-2 py-1 text-xs text-amber-100">
      {humanizeForgeMessage(actionError || undertakings.error || "")}
    </p>
  {/if}

  <div class="grid min-h-0 flex-1 gap-3 {showBrowser ? 'lg:grid-cols-[240px_1fr]' : 'grid-cols-1'}">
    {#if showBrowser}<aside class="flex min-h-0 flex-col gap-2 overflow-auto border-r border-surface-500/25 pr-3">
      {#if creating}
        <form
          class="flex flex-col gap-1.5 rounded-lg border border-surface-500/35 bg-surface-900/35 p-2"
          onsubmit={(e) => {
            e.preventDefault();
            void onCreate();
          }}
        >
          <p class="px-0.5 text-[10px] font-medium uppercase tracking-wide text-surface-400">
            New project
          </p>
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder="What are you changing?"
          aria-label="Project name"
          bind:value={title}
        />
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder="What should be true when it’s done?"
          aria-label="Goal"
          bind:value={brief}
        />
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder={isCoLocatedWorkshop()
            ? "Repository folder"
            : "Repository folder on connected computer"}
          aria-label="Repository folder"
          bind:value={repoPath}
        />
        <input
          class="rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
          placeholder="Starting branch"
          aria-label="Starting branch"
          bind:value={baseRef}
        />
          <button
            type="submit"
            class="rounded bg-primary-500/80 px-2 py-1 text-xs font-medium text-surface-50 disabled:opacity-40"
            disabled={busy || !title.trim() || !repoPath.trim()}
          >
            Start project
          </button>
        </form>
      {/if}
      {#if undertakings.loading && undertakings.items.length === 0}
        <p class="text-xs text-surface-500">Loading…</p>
      {:else if undertakings.items.length === 0 && !creating}
        <p class="px-1 py-3 text-xs leading-relaxed text-surface-500">
          No code projects yet. Start with a repository and the change you want to make.
        </p>
      {/if}
      <ul class="flex flex-col gap-1">
        {#if activeItems.length}
          <li class="px-1 pt-1 text-[10px] uppercase tracking-wide text-surface-500">
            In progress
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
                  {humanPhaseLabel(item.human_phase)}
                </span>
              </button>
            </li>
          {/each}
        {/if}
        {#if completedItems.length}
          <li class="px-1 pt-2 text-[10px] uppercase tracking-wide text-surface-500">
            Finished
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
                  {humanPhaseLabel(item.human_phase)}
                </span>
              </button>
            </li>
          {/each}
        {/if}
      </ul>
    </aside>{/if}

    <section class="flex min-h-0 flex-col gap-2 px-1 py-2 {showBrowser ? 'overflow-auto' : 'overflow-hidden'}">
      {#if !detail}
        <div class="flex min-h-48 flex-1 items-center justify-center">
          <div class="max-w-sm text-center">
            <p class="text-sm font-medium text-surface-300">Choose what you want to change</p>
            <p class="mt-1 text-xs leading-relaxed text-surface-500">
              Medousa keeps the files, conversations, tools, and agents together so you can stay focused on the outcome.
            </p>
          </div>
        </div>
      {:else}
        <div class="flex flex-wrap items-start justify-between gap-2">
          <div>
            <h3 class="text-lg font-semibold text-surface-50">{detail.title}</h3>
            <p class="text-xs text-surface-400">{detail.brief}</p>
            <p class="mt-1 text-[11px] text-surface-400">
              <span class="font-medium text-surface-300">{humanPhaseLabel(detail.human_phase)}</span>
              · {humanPhaseGuidance(detail.human_phase)}
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
                Set up project
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
            {:else if actions?.start_agent.allowed}
              <button
                type="button"
                class="rounded-md bg-primary-500/80 px-3 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void startAgent(preferredCodeAgent)}
              >
                Continue with {preferredCodeAgent === "codex" ? "Codex" : "Cursor"}
              </button>
            {:else if actions?.open_terminal.allowed}
              <button
                type="button"
                class="rounded-md bg-primary-500/80 px-3 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void openTerminalTracked()}
              >
                Open Terminal
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
                >Ask Codex to continue</button>
                <button
                  type="button"
                  class="secondary-action"
                  disabled={busy || !actions?.start_agent.allowed}
                  onclick={() => void startAgent("cursor")}
                >Ask Cursor to continue</button>
                <button
                  type="button"
                  class="secondary-action"
                  disabled={busy || !actions?.open_terminal.allowed}
                  onclick={() => void openTerminalTracked()}
                >Open another Terminal</button>
                <div class="my-1 border-t border-surface-500/25"></div>
                <button
                  type="button"
                  class="secondary-action text-rose-200"
                  disabled={busy || !actions?.discard.allowed}
                  onclick={() => void discardWithConfirmation()}
                >Discard project…</button>
              </div>
            </details>
          </div>
        </div>

        {#if detail.environment}
          <details class="text-[10px] text-surface-500">
            <summary class="w-fit cursor-pointer select-none hover:text-surface-300">
              Technical details
            </summary>
            <p class="mt-1 break-all font-mono">
              Working copy: {detail.environment.worktree}<br />Starting revision:
              {detail.environment.baseline_oid.slice(0, 12)} · internal state: {detail.state}
            </p>
          </details>
        {/if}

        {#if undertakings.active?.workId === detail.id && undertakings.active.selectedPath}
          <div
            class="flex flex-wrap items-center justify-between gap-2 rounded-md border border-primary-500/20 bg-primary-950/10 px-2.5 py-2"
          >
            <div class="min-w-0">
              <p class="truncate font-mono text-[11px] text-surface-200">
                {undertakings.active.selectedPath}{undertakings.active.selectedLine
                  ? `:${undertakings.active.selectedLine}`
                  : ""}
              </p>
              <p class="text-[9px] text-surface-500">
                Current focus{undertakings.active.selectedEntityId
                  ? " · code relationship found"
                  : ""}
              </p>
            </div>
            <div class="flex items-center gap-1">
              <button
                type="button"
                class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100"
                onclick={() => void copyLocationLink()}
              >Copy link</button>
              <button
                type="button"
                class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100"
                onclick={() => undertakings.setSelection({ path: null, line: null, entityId: null })}
              >Clear</button>
            </div>
          </div>
        {/if}

        {#if detail.environment}
          <CodeSourceEditor
            fill={!showBrowser}
            worldOpen={worldMode}
            reviewAvailable={Boolean(review && (detail.human_phase === "review" || review.evidence_id))}
            terminalAvailable={Boolean(actions?.open_terminal.allowed)}
            preferredAgent={preferredCodeAgent}
            onToggleWorld={() => void toggleWorldFromEditor()}
            onOpenReview={() => void openReviewFromEditor()}
            onOpenTerminal={() => void openTerminalTracked()}
            onHandoffToAgent={handoffToAgent}
            onReclaimHuman={reclaimHuman}
          />
        {/if}

        {#if review && (detail.human_phase === "review" || review.evidence_id)}
          <div bind:this={reviewEl} class="mt-2 rounded-lg border border-primary-500/30 bg-surface-900/50 p-3 {showBrowser ? '' : 'max-h-[45%] shrink-0 overflow-auto'}">
            <div class="flex items-center justify-between gap-2">
              <div>
                <h4 class="text-sm font-semibold text-surface-50">Review changes</h4>
                <p class="text-[10px] text-surface-500">Everything that changed, in one place</p>
              </div>
              <button
                type="button"
                class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100"
                onclick={() => void beginExport()}
              >Save project record…</button>
            </div>
            <details class="mt-1 text-[10px] text-surface-500">
              <summary class="w-fit cursor-pointer hover:text-surface-300">Technical details</summary>
              <p class="mt-1 font-mono">
                Starting revision {review.baseline_oid?.slice(0, 10)}… → reviewed revision
                {review.sealed_head_oid?.slice(0, 10)}…
                {#if review.evidence_digest} · record {review.evidence_digest.slice(0, 16)}…{/if}
                {#if review.truncated} · preview shortened{/if}
                {#if review.base_advanced} · starting branch changed{/if}
              </p>
            </details>
            <ForgeReviewSurface
              {review}
              {busy}
              onOpenFile={(path, line) => revealLocation({ path, line })}
              onRestore={restoreReviewedFile}
            />
            {#if providerHandoff}
              <div class="mt-2 rounded-md border border-surface-500/25 bg-surface-950/30 p-2">
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <p class="text-[11px] font-medium text-surface-200">Share this work</p>
                    <p class="text-[10px] text-surface-500">
                      {#if providerHandoff.provider === "github"}GitHub{:else if providerHandoff.provider === "gitlab"}GitLab{:else}Repository provider{/if}
                      {#if providerHandoff.repository} · {providerHandoff.repository}{/if}
                    </p>
                  </div>
                  <div class="flex items-center gap-1">
                    {#if providerHandoff.review_url}
                      <button type="button" class="rounded px-2 py-1 text-[10px] text-primary-300 hover:bg-surface-800" onclick={() => window.open(providerHandoff?.review_url ?? "", "_blank", "noopener,noreferrer")}>Open review</button>
                    {/if}
                    {#if providerHandoff.available && (detail.state === "awaiting_review" || detail.state === "accepted")}
                      <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-white disabled:opacity-40" disabled={busy} onclick={() => void shareProject()}>
                        {providerHandoff.review_url ? "Update review" : "Share branch and open review"}
                      </button>
                    {/if}
                  </div>
                  {#if !providerHandoff.available}
                    <p class="max-w-sm text-right text-[9px] text-surface-500">{providerHandoff.message}</p>
                  {/if}
                </div>
                {#if providerHandoff.links.length}
                  <div class="mt-2 flex flex-wrap gap-1">
                    {#each providerHandoff.links as link (link)}
                      <button type="button" class="max-w-full truncate rounded bg-surface-800 px-1.5 py-0.5 text-[9px] text-surface-300 hover:text-white" onclick={() => window.open(link, "_blank", "noopener,noreferrer")}>{link}</button>
                    {/each}
                  </div>
                {/if}
                <div class="mt-2 flex gap-1">
                  <input class="min-w-0 flex-1 rounded border border-surface-500/30 bg-surface-950 px-2 py-1 text-[10px] text-surface-200" type="url" placeholder="Link an issue, PR, or ticket" bind:value={providerLink} onkeydown={(event) => { if (event.key === "Enter") { event.preventDefault(); void addProviderLink(); } }} />
                  <button type="button" class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 disabled:opacity-40" disabled={!providerLink.trim() || busy} onclick={() => void addProviderLink()}>Link</button>
                </div>
                {#if providerHandoff.review_url && providerHandoff.provider === "github"}
                  <button type="button" class="mt-2 text-[10px] text-surface-400 hover:text-surface-200" onclick={() => void loadProviderComments()}>{providerOpen ? "Hide review feedback" : "Review feedback"}</button>
                  {#if providerOpen}
                    <div class="mt-1 divide-y divide-surface-500/15 rounded border border-surface-500/20">
                      {#if providerComments.length === 0}
                        <p class="px-2 py-2 text-[10px] text-surface-500">No review feedback yet.</p>
                      {/if}
                      {#each providerComments as comment (comment.id)}
                        <div class="p-2">
                          <p class="text-[9px] text-surface-500">{comment.author}</p>
                          <p class="mt-0.5 whitespace-pre-wrap text-[10px] text-surface-300">{comment.body}</p>
                          <button type="button" class="mt-1 text-[9px] text-primary-300 hover:underline" disabled={busy} onclick={() => void createFollowUp(comment)}>Make this a follow-up project</button>
                        </div>
                      {/each}
                    </div>
                  {/if}
                {/if}
              </div>
            {/if}
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
                        These changes exceed the configured size limit
                      {/if}
                    </li>
                  {/each}
                </ul>
                {#if review.policy.violations.length}
                  <label class="mt-2 flex items-start gap-2 text-[10px] text-amber-50">
                    <input type="checkbox" class="mt-0.5" bind:checked={acknowledgePolicy} />
                    I reviewed these exceptions and accept them for this change.
                  </label>
                {/if}
              </div>
            {/if}
            {#if patch || (commands && commands.lines.length)}
              <details class="mt-2 rounded-md border border-surface-500/20 bg-surface-950/20 p-2">
                <summary class="cursor-pointer text-[10px] text-surface-400 hover:text-surface-200">Raw evidence</summary>
                {#if patch}
                  <pre class="mt-2 max-h-48 overflow-auto rounded bg-black/40 p-2 text-[10px] leading-snug text-surface-200">{patch.lines.join("\n")}</pre>
                  {#if patch.truncated}
                    <button type="button" class="mt-1 text-[10px] text-primary-300 hover:underline disabled:opacity-40" disabled={busy} onclick={() => void loadMorePatch()}>Show more changes · {patch.lines.length} of {patch.total_lines} lines</button>
                  {/if}
                {/if}
                {#if commands && commands.lines.length}
                  <p class="mt-2 text-[10px] font-medium text-surface-300">Command record</p>
                  <pre class="mt-1 max-h-32 overflow-auto rounded bg-black/40 p-2 text-[10px] text-surface-300">{commands.lines.join("\n")}</pre>
                  {#if commands.truncated}
                    <button type="button" class="mt-1 text-[10px] text-primary-300 hover:underline disabled:opacity-40" disabled={busy} onclick={() => void loadMoreCommands()}>Show more commands · {commands.lines.length} of {commands.total_lines}</button>
                  {/if}
                {/if}
              </details>
            {/if}
            {#if worldInsight}
              <div class="mt-2 rounded-md bg-surface-950/35 p-2">
                <p class="text-[11px] font-medium text-surface-300">Code understanding</p>
                {#if worldInsight.code_avec}
                  <p class="mt-1 text-[10px] text-surface-400">
                    {worldInsight.code_avec.fully_scored_entities} of
                    {worldInsight.code_avec.scoreable_entities} known code elements are fully understood.
                  </p>
                {/if}
              </div>
            {/if}
            {#if actions?.review.allowed}
              <label class="mt-3 block text-[10px] text-surface-400" for="review-rationale">
                Review note <span class="text-surface-600">(optional)</span>
              </label>
              <textarea
                id="review-rationale"
                rows="2"
                class="mt-1 w-full resize-none rounded-md border border-surface-500/40 bg-surface-950/50 px-2 py-1.5 text-xs text-surface-100 placeholder:text-surface-600"
                placeholder="Anything the next person should know?"
                bind:value={reviewRationale}
              ></textarea>
              <button
                type="button"
                class="mt-2 rounded bg-primary-500/80 px-2.5 py-1.5 text-xs font-medium disabled:opacity-40"
                disabled={busy || (!!review.policy?.violations.length && !acknowledgePolicy)}
                onclick={() => void recordApproval()}
              >
                Approve changes
              </button>
            {:else if actions?.apply.allowed}
              <div class="mt-3 flex items-center justify-between gap-3 rounded-md border border-primary-500/25 bg-primary-950/15 p-2">
                <p class="text-[11px] text-surface-300">
                  Approved. Finish when you are ready to keep this work.
                </p>
                <button
                  type="button"
                  class="shrink-0 rounded bg-primary-500/80 px-2.5 py-1.5 text-xs font-medium disabled:opacity-40"
                  disabled={busy}
                  onclick={() => void applyApproval()}
                >
                  Finish project…
                </button>
              </div>
            {/if}
            {#if exportedDestination}
              <p class="mt-2 text-[10px] text-primary-200">
                Project record saved at <span class="font-mono">{exportedDestination}</span>
              </p>
            {/if}
          </div>
        {/if}

        {#if exportOpen}
          <div
            class="rounded-lg border border-surface-500/35 bg-surface-900/60 p-3"
            role="dialog"
            aria-label="Save project record"
            tabindex="-1"
          >
            <h4 class="text-sm font-medium text-surface-100">Preserve a portable copy</h4>
            <p class="mt-1 text-[10px] leading-relaxed text-surface-400">
              This creates a portable folder with what changed, how it was made, and the decisions you recorded.
            </p>
            {#if !isCoLocatedWorkshop()}
              <label class="mt-3 block text-[10px] text-surface-400" for="export-destination">
                Save on the connected computer
              </label>
              <input
                id="export-destination"
                class="mt-1 w-full rounded-md border border-surface-500/40 bg-surface-950/60 px-2 py-1.5 font-mono text-xs text-surface-100"
                placeholder="/path/on/connected-computer/project-record"
                bind:value={exportDestination}
              />
              <p class="mt-1 text-[9px] text-surface-500">
                Files stay on the connected computer. Nothing is uploaded from this device.
              </p>
            {:else}
              <p class="mt-3 break-all font-mono text-[10px] text-surface-300">
                {exportDestination}
              </p>
            {/if}
            <div class="mt-3 flex justify-end gap-1.5">
              <button
                type="button"
                class="rounded px-2.5 py-1.5 text-xs text-surface-400 hover:bg-surface-800"
                onclick={() => (exportOpen = false)}
              >Cancel</button>
              <button
                type="button"
                class="rounded bg-primary-500/80 px-2.5 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy || !exportDestination.trim()}
                onclick={() => void confirmExport()}
              >Save copy</button>
            </div>
          </div>
        {/if}

        {#if worldMode}
          <div bind:this={worldEl} class="rounded-lg border border-surface-500/40 p-3 {showBrowser ? '' : 'max-h-[45%] shrink-0 overflow-auto'}">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h4 class="text-sm font-semibold">Understand this code</h4>
                <p class="text-[10px] text-surface-500">See relationships and possible impact without leaving your work</p>
              </div>
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
                  Before
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
                  Current
                </button>
              </div>
            </div>
            <p class="mt-1 text-[10px] text-surface-500">This view only explains the code; it never changes files.</p>
            <div class="mt-2 flex flex-wrap gap-1">
              <button
                type="button"
                class="rounded border border-surface-500/50 px-2 py-1 text-xs"
                onclick={() => void loadWorldOverview()}
              >
                Refresh understanding
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
                Rebuild code map
              </button>
            </div>
            {#if worldBinding}
              <details class="mt-2 text-[10px] text-surface-500">
                <summary class="w-fit cursor-pointer hover:text-surface-300">Technical details</summary>
                <p class="mt-1">
                  Before: {worldBinding.baseline?.state ?? "not indexed"} · current:
                  {worldBinding.sealed?.state ?? "not indexed"}
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
              </details>
            {/if}
            <div class="mt-2 flex flex-wrap items-center gap-1">
              <input
                class="min-w-[120px] flex-1 rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
                placeholder="Find a class, function, or name…"
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
                placeholder="Class or function to check"
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
                See impact
              </button>
            </div>
            {#if worldFind}
              <div class="mt-2 max-h-44 overflow-auto rounded-md border border-surface-500/25">
                {#if worldFind.entities.length === 0}
                  <p class="p-2 text-[10px] text-surface-500">Nothing matched that name.</p>
                {:else}
                  {#each worldFind.entities as entity (entity.id)}
                    <button
                      type="button"
                      class="flex w-full items-center justify-between gap-2 border-b border-surface-500/20 px-2 py-1.5 text-left last:border-0 hover:bg-surface-800/60"
                      onclick={() => {
                        void revealLocation({
                          path: entity.path,
                          line: entity.line_start,
                          entityId: entity.id,
                        });
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
                  What depends on this · {worldImpact.direct_dependents ?? 0} directly,
                  {worldImpact.transitive_dependents ?? 0} through other code
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
                  <p class="text-[9px] text-surface-500">fully understood</p>
                </div>
                <div class="rounded-md bg-surface-900/60 p-2">
                  <p class="text-lg font-semibold text-surface-100">
                    {worldInsight.code_avec?.scoreable_entities ?? 0}
                  </p>
                  <p class="text-[9px] text-surface-500">code elements found</p>
                </div>
                <div class="rounded-md bg-surface-900/60 p-2">
                  <p class="text-lg font-semibold text-surface-100">
                    {worldInsight.code_avec?.gaps.length ?? 0}
                  </p>
                  <p class="text-[9px] text-surface-500">still unclear</p>
                </div>
              </div>
            {/if}
            {#if worldFiles}
              <details class="mt-2">
                <summary class="cursor-pointer text-[10px] text-surface-400">
                  Files in this view · {worldFiles.files.length}
                </summary>
                <ul class="mt-1 max-h-48 overflow-auto rounded-md border border-surface-500/25">
                  {#each worldFiles.files as file (file.id)}
                    <li class="border-b border-surface-500/15 px-2 py-1 last:border-0">
                      <button
                        type="button"
                        class="w-full truncate text-left font-mono text-[10px] text-surface-400 hover:text-surface-100"
                        onclick={() =>
                          void revealLocation({ path: file.path, line: 1, entityId: file.id })}
                      >{file.path}</button>
                    </li>
                  {/each}
                </ul>
              </details>
            {/if}
            {#if worldError}
              <p class="mt-2 rounded-md bg-amber-950/30 p-2 text-[10px] text-amber-100">
                Code understanding is not ready yet. {humanizeForgeMessage(worldError)}
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
