<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    Check,
    ChevronRight,
    CircleAlert,
    GitCommitHorizontal,
    GitPullRequestArrow,
    History,
    Link2,
    MessageSquarePlus,
    MoreHorizontal,
    Play,
    Save,
    ShieldCheck,
    Square,
    UserRound,
  } from "@lucide/svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import {
    lmeWorkspace,
    type CodeWorkspaceResource,
  } from "$lib/stores/lmeWorkspace.svelte";
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
    interruptTrackedAgent,
    landCodeWorkingSet,
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
  import { isTauri } from "$lib/window";
  import { toast } from "$lib/stores/toast.svelte";

  interface Props {
    /** The Workspace Code explorer owns creation and undertaking selection. */
    showBrowser?: boolean;
    /** Shell-bound Code resource. The shell tab, not this panel, owns navigation. */
    workId?: string;
    resource?: CodeWorkspaceResource;
    /** Background split panes render but must not own global interactions. */
    interactive?: boolean;
  }

  let {
    showBrowser = true,
    workId: boundWorkId = "",
    resource = { kind: "workspace" },
    interactive = true,
  }: Props = $props();

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
  let reviewNoteOpen = $state(false);
  let acknowledgePolicy = $state(false);
  let exportOpen = $state(false);
  let exportDestination = $state("");
  let exportedDestination = $state<string | null>(null);
  let worldEl = $state<HTMLDivElement | null>(null);
  let preferredCodeAgent = $state<"codex" | "cursor">("codex");
  let providerHandoff = $state<ProviderHandoff | null>(null);
  let providerComments = $state<ProviderComment[]>([]);
  let providerLink = $state("");
  let providerOpen = $state(false);
  let reviewDetailsFor = $state<string | null>(null);
  let reviewDetailsLoading = $state(false);
  let reviewDetailsOpen = $state(false);
  let reviewTimelineOpen = $state(false);
  let reviewAuditOpen = $state(false);
  let providerLinkOpen = $state(false);

  const detail = $derived(
    !boundWorkId || undertakings.detail?.id === boundWorkId
      ? undertakings.detail
      : null,
  );
  const review = $derived(
    !boundWorkId || undertakings.review?.work_id === boundWorkId
      ? undertakings.review
      : null,
  );
  const reviewCanvas = $derived(resource.kind === "review");
  const resourcePath = $derived(
    resource.kind === "file" ? resource.path : null,
  );
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
    // Default repo path only when Home shares the workshop disk.
    if (isCoLocatedWorkshop()) {
      const root = vault.activeVaultRoot;
      if (root?.path) repoPath = root.path;
    }
    const savedAgent = localStorage.getItem("medousa-code-agent-runtime");
    if (savedAgent === "cursor" || savedAgent === "codex") preferredCodeAgent = savedAgent;
  });

  $effect(() => {
    if (!interactive) return;
    void undertakings.refreshList();
    undertakings.startPolling();
    return () => undertakings.stopPolling();
  });

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
  }

  async function openReviewFromEditor() {
    if (!detail) return;
    await lmeWorkspace.openCodeReview(detail.id, `Review · ${detail.title}`);
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

  async function interruptAgent() {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await interruptTrackedAgent(d);
      await undertakings.refreshDetail();
    });
  }

  async function selectProject(id: string) {
    const item = undertakings.items.find((entry) => entry.id === id);
    await lmeWorkspace.openCodeWorkspace(id, item?.title);
  }

  async function provisionAndOpenProject(workId: string) {
    await undertakings.provision(workId);
    const landed = await landCodeWorkingSet(workId);
    if (landed.ok) {
      await lmeWorkspace.openCodeFile(workId, landed.path, { line: 1 });
    }
  }

  async function revealWorktree() {
    const worktree = detail?.environment?.worktree?.trim();
    if (!worktree) return;
    if (!isTauri() || !isCoLocatedWorkshop()) {
      toast.show("Reveal needs a local workshop on this device.");
      return;
    }
    try {
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      await revealItemInDir(worktree);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    }
  }

  const agentRunning = $derived(
    Boolean(
      undertakings.active?.workId === detail?.id &&
        undertakings.active?.executorKind &&
        undertakings.active.executorKind !== "human",
    ),
  );
  const agentLabel = $derived(
    undertakings.active?.executorKind === "cursor" ? "Cursor" : "Codex",
  );
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

  async function loadReviewDetails() {
    const current = review;
    if (!current?.evidence_id || reviewDetailsFor === current.evidence_id) return;
    const evidenceId = current.evidence_id;
    reviewDetailsFor = evidenceId;
    reviewDetailsLoading = true;
    patch = null;
    commands = null;
    worldInsight = null;
    providerHandoff = null;
    const [patchResult, commandsResult, worldResult, providerResult] = await Promise.allSettled([
      getEvidencePatch(evidenceId, { work_id: current.work_id, limit: 400 }),
      getEvidenceCommands(evidenceId, { work_id: current.work_id, limit: 100 }),
      getWorldCodeAvec(current.work_id),
      getProviderHandoff(current.work_id),
    ]);
    if (review?.evidence_id !== evidenceId) return;
    patch = patchResult.status === "fulfilled" ? patchResult.value : null;
    commands = commandsResult.status === "fulfilled" ? commandsResult.value : null;
    if (worldResult.status === "fulfilled") {
      worldInsight = worldResult.value;
      worldError = null;
    } else {
      worldError =
        worldResult.reason instanceof Error
          ? worldResult.reason.message
          : String(worldResult.reason);
    }
    providerHandoff = providerResult.status === "fulfilled" ? providerResult.value : null;
    reviewDetailsLoading = false;
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

  function reviewIssueLabel(issue: string): string {
    if (issue.toLowerCase().includes("no project check")) {
      return "Verification is missing";
    }
    return humanizeForgeMessage(issue);
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
        attempt_id: comparison.attempt_id,
      });
      undertakings.setActiveFromItem(result.item);
      await undertakings.refreshDetail();
      await lmeWorkspace.openCodeFile(detail!.id, result.path, { line: 1 });
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

  async function shareProject() {
    if (!detail) return;
    await run(async () => {
      providerHandoff = await shareProviderHandoff(detail!.id, {
        title: detail!.title,
        attempt_id: review?.attempt_id ?? undefined,
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
      await selectProject(item.id);
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
                onclick={() => void selectProject(item.id)}
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
                onclick={() => void selectProject(item.id)}
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

    <section class="relative flex min-h-0 flex-col {showBrowser ? 'gap-2 overflow-auto px-1 py-2' : 'overflow-hidden'}">
      {#if !detail}
        <div class="flex min-h-48 flex-1 items-center justify-center px-1 py-2">
          <div class="max-w-sm text-center">
            <p class="text-sm font-medium text-surface-300">Open or start a project</p>
            <p class="mt-1 text-xs leading-relaxed text-surface-500">
              Pick a project in the rail, or start one with the outcome you want.
            </p>
          </div>
        </div>
      {:else}
        <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 {showBrowser ? 'px-0' : 'border-b border-surface-500/25 px-2 py-1'}">
          <div class="min-w-0 flex-1">
            <h3 class="truncate text-sm font-semibold text-surface-50" title={detail.brief || detail.title}>{detail.title}</h3>
            {#if detail.environment}
              <p class="mt-0.5 text-[10px] text-surface-500">
                {humanPhaseLabel(detail.human_phase)}
              </p>
            {:else}
              {#if detail.brief && detail.brief.trim() !== detail.title.trim()}
                <p class="truncate text-[10px] text-surface-500" title={detail.brief}>{detail.brief}</p>
              {/if}
              <p class="mt-0.5 text-[10px] text-surface-500">
                <span class="font-medium text-surface-300">{humanPhaseLabel(detail.human_phase)}</span>
                · {humanPhaseGuidance(detail.human_phase)}
              </p>
            {/if}
          </div>
          <div class="flex shrink-0 items-center gap-1.5">
            {#if !detail.environment && actions?.provision.allowed}
              <button
                type="button"
                class="rounded-md bg-primary-500/80 px-3 py-1.5 text-xs font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void run(async () => {
                  await provisionAndOpenProject(detail.id);
                })}
              >
                Set up project
              </button>
            {:else if actions?.seal.allowed}
              <button
                type="button"
                class="scripts-workbench-toolbar-btn flex items-center gap-1 text-amber-300/85"
                disabled={busy}
                onclick={() => void doSeal()}
                title="Review changes"
              >
                <GitPullRequestArrow size={14} strokeWidth={1.75} />
                <span class="hidden sm:inline">Review</span>
              </button>
            {/if}

            {#if agentRunning}
              <button
                type="button"
                class="scripts-workbench-toolbar-btn flex items-center gap-1 text-amber-300"
                disabled={busy}
                onclick={() => void interruptAgent()}
                title={`Stop ${agentLabel}`}
              ><Square size={13} /><span class="hidden sm:inline">Stop</span></button>
              <button
                type="button"
                class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary flex items-center gap-1"
                disabled={busy}
                onclick={() => void reclaimHuman()}
                title="Resume editing"
              ><Play size={14} /><span class="hidden sm:inline">Resume</span></button>
            {/if}

            <details class="relative">
              <summary
                class="scripts-workbench-toolbar-btn cursor-pointer list-none [&::-webkit-details-marker]:hidden"
                title="Project actions"
                aria-label="Project actions"
              >
                <MoreHorizontal size={15} strokeWidth={1.75} />
              </summary>
              <div
                class="absolute right-0 top-full z-30 mt-1 w-52 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
              >
                {#if actions?.start_agent.allowed}
                  <button
                    type="button"
                    class="secondary-action"
                    disabled={busy}
                    title={actions?.start_agent.reason ?? ""}
                    onclick={() => void startAgent("codex")}
                  >Ask Codex to continue</button>
                  <button
                    type="button"
                    class="secondary-action"
                    disabled={busy}
                    onclick={() => void startAgent("cursor")}
                  >Ask Cursor to continue</button>
                {/if}
                <button
                  type="button"
                  class="secondary-action"
                  disabled={busy || !actions?.open_terminal.allowed}
                  onclick={() => void openTerminalTracked()}
                >Terminal in working copy</button>
                {#if detail.environment?.worktree}
                  <button
                    type="button"
                    class="secondary-action"
                    disabled={busy}
                    onclick={() => void revealWorktree()}
                  >Reveal working copy</button>
                {/if}
                {#if undertakings.active?.workId === detail.id && undertakings.active.selectedPath}
                  <button
                    type="button"
                    class="secondary-action"
                    disabled={busy}
                    onclick={() => void copyLocationLink()}
                  >Copy location link</button>
                {/if}
                <div class="my-1 border-t border-surface-500/25"></div>
                {#if detail.environment}
                  <details class="px-2 py-1 text-[9px] text-surface-500">
                    <summary class="cursor-pointer select-none hover:text-surface-300">Technical details</summary>
                    <p class="mt-1 break-all font-mono leading-relaxed">
                      Working copy: {detail.environment.worktree}<br />Starting revision:
                      {detail.environment.baseline_oid.slice(0, 12)} · internal state: {detail.state}
                    </p>
                  </details>
                {/if}
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

        {#if detail.environment && !reviewCanvas}
          <CodeSourceEditor
            fill={!showBrowser}
            workId={detail.id}
            {resourcePath}
            {interactive}
            worldOpen={worldMode}
            reviewAvailable={Boolean(review && (detail.human_phase === "review" || review.evidence_id))}
            terminalAvailable={Boolean(actions?.open_terminal.allowed)}
            preferredAgent={preferredCodeAgent}
            onToggleWorld={() => void toggleWorldFromEditor()}
            onOpenReview={() => void openReviewFromEditor()}
            onOpenTerminal={() => void openTerminalTracked()}
            onProvision={async () => {
              await run(async () => {
                await provisionAndOpenProject(detail.id);
              });
            }}
            onHandoffToAgent={handoffToAgent}
            onReclaimHuman={reclaimHuman}
          />
        {:else if !reviewCanvas && actions?.provision.allowed}
          <div class="flex min-h-48 flex-1 items-center justify-center p-6 text-center">
            <div class="max-w-sm">
              <p class="text-xs font-medium text-surface-300">Set up this project</p>
              <p class="mt-1 text-[10px] leading-relaxed text-surface-500">
                Create the working copy so the tree and editor can open.
              </p>
              <button
                type="button"
                class="mt-3 rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50 disabled:opacity-40"
                disabled={busy}
                onclick={() => void run(async () => {
                  await provisionAndOpenProject(detail.id);
                })}
              >Set up project</button>
            </div>
          </div>
        {:else if !reviewCanvas && !actions?.provision.allowed}
          <div class="flex min-h-48 flex-1 items-center justify-center p-6 text-center">
            <p class="max-w-sm text-xs text-surface-500">
              {humanPhaseGuidance(detail.human_phase)}
            </p>
          </div>
        {/if}

        {#if reviewCanvas && review && (detail.human_phase === "review" || review.evidence_id)}
          <div class="flex min-h-0 flex-1 flex-col bg-surface-950/20">
            <div class="min-h-0 flex-1 overflow-auto px-4 py-3">
              <div class="review-canvas">
                <ForgeReviewSurface
                  {review}
                  {busy}
                  onOpenFile={(path, line) => revealLocation({ path, line })}
                  onRestore={restoreReviewedFile}
                  onSelectCandidate={(attemptId) => undertakings.selectReviewAttempt(attemptId)}
                />

                {#if review.policy && (review.policy.violations.length || review.policy.capture_risks.length)}
                  <section class="review-policy" aria-label="Policy exceptions">
                    <CircleAlert size={15} strokeWidth={1.7} aria-hidden="true" />
                    <div class="min-w-0 flex-1">
                      <p>Review exceptions</p>
                      <ul>
                        {#each review.policy.violations as violation (violation.id)}
                          <li><span>{violation.path}</span> — {violation.detail}</li>
                        {/each}
                        {#each review.policy.capture_risks as risk}
                          <li>
                            {#if risk.kind === "secret_pattern"}
                              Possible secret in <span>{risk.path}</span>
                            {:else if risk.kind === "oversize_file"}
                              Large file <span>{risk.path}</span>
                            {:else}
                              These changes exceed the configured size limit
                            {/if}
                          </li>
                        {/each}
                      </ul>
                      {#if review.policy.violations.length}
                        <label>
                          <input type="checkbox" bind:checked={acknowledgePolicy} />
                          <span>I reviewed and accept these exceptions.</span>
                        </label>
                      {/if}
                    </div>
                  </section>
                {/if}

                <section class="review-context">
                  <button
                    type="button"
                    class="review-context-disclosure"
                    aria-expanded={reviewDetailsOpen}
                    onclick={() => {
                      reviewDetailsOpen = !reviewDetailsOpen;
                      if (reviewDetailsOpen) void loadReviewDetails();
                    }}
                  >
                    <ChevronRight
                      size={13}
                      strokeWidth={2}
                      class="review-context-chevron {reviewDetailsOpen ? 'review-context-chevron--open' : ''}"
                    />
                    <span>About this review</span>
                    <small>People, history, sharing, and recovery</small>
                  </button>

                  {#if reviewDetailsOpen}
                    <div class="review-context-body">
                      {#if reviewDetailsLoading}
                        <div class="review-context-loading">
                          <span class="review-context-loading-dot"></span>
                          Preparing context…
                        </div>
                      {/if}

                      <div class="review-context-row">
                        <span class="review-context-icon"><UserRound size={14} strokeWidth={1.7} /></span>
                        <div class="review-context-copy">
                          <p>Contributors</p>
                          <span>
                            {#if review.attribution.length}
                              {review.attribution.map((source) => source.label).join(", ")}
                            {:else}
                              No contributor record
                            {/if}
                          </span>
                        </div>
                      </div>

                      <div class="review-context-row">
                        <span class="review-context-icon"><GitCommitHorizontal size={14} strokeWidth={1.7} /></span>
                        <div class="review-context-copy">
                          <p>Recovery point</p>
                          <span>
                            The reviewed state is saved and can be revisited later.
                            {#if review.base_advanced} The starting branch moved while this work was open.{/if}
                          </span>
                        </div>
                        <button type="button" class="review-context-action" onclick={() => void beginExport()}>
                          <Save size={12} />Save a copy…
                        </button>
                      </div>

                      <div class="review-context-row review-context-row--stack">
                        <button
                          type="button"
                          class="review-context-row-button"
                          aria-expanded={reviewTimelineOpen}
                          onclick={() => (reviewTimelineOpen = !reviewTimelineOpen)}
                        >
                          <span class="review-context-icon"><History size={14} strokeWidth={1.7} /></span>
                          <span class="review-context-copy">
                            <span class="review-context-copy-title">Project history</span>
                            <span>{review.timeline.length} recorded {review.timeline.length === 1 ? "event" : "events"}</span>
                          </span>
                          <ChevronRight
                            size={13}
                            class="review-context-row-chevron {reviewTimelineOpen ? 'review-context-row-chevron--open' : ''}"
                          />
                        </button>
                        {#if reviewTimelineOpen}
                          <ol class="review-timeline">
                            {#each review.timeline as event (event.id)}
                              <li>
                                <span class="review-timeline-dot"></span>
                                <div>
                                  <p>{event.label}</p>
                                  <span>{event.actor_label}{event.detail ? ` · ${event.detail}` : ""}</span>
                                </div>
                                <time>{new Date(event.at).toLocaleString()}</time>
                              </li>
                            {/each}
                          </ol>
                        {/if}
                      </div>

                      {#if providerHandoff}
                        <div class="review-context-row review-context-row--stack">
                          <div class="review-context-row-main">
                            <span class="review-context-icon"><Link2 size={14} strokeWidth={1.7} /></span>
                            <div class="review-context-copy">
                              <p>Repository review</p>
                              <span>
                                {#if providerHandoff.available}
                                  {#if providerHandoff.repository}{providerHandoff.repository}{:else}Ready to share{/if}
                                {:else}
                                  Sharing is not configured on this workshop.
                                {/if}
                              </span>
                            </div>
                            {#if providerHandoff.review_url}
                              <button type="button" class="review-context-action" onclick={() => window.open(providerHandoff?.review_url ?? "", "_blank", "noopener,noreferrer")}>Open review</button>
                            {:else if providerHandoff.available && (detail.state === "awaiting_review" || detail.state === "accepted")}
                              <button type="button" class="review-context-action" disabled={busy} onclick={() => void shareProject()}>Share branch…</button>
                            {/if}
                          </div>

                          {#if providerHandoff.available}
                            {#if providerHandoff.links.length}
                              <div class="review-linked-items">
                                {#each providerHandoff.links as link (link)}
                                  <button type="button" onclick={() => window.open(link, "_blank", "noopener,noreferrer")}>{link}</button>
                                {/each}
                              </div>
                            {/if}
                            <div class="review-context-subactions">
                              <button type="button" onclick={() => (providerLinkOpen = !providerLinkOpen)}>
                                {providerLinkOpen ? "Cancel" : "Add related link"}
                              </button>
                              {#if providerHandoff.review_url && providerHandoff.provider === "github"}
                                <button type="button" onclick={() => void loadProviderComments()}>
                                  {providerOpen ? "Hide feedback" : "Load feedback"}
                                </button>
                              {/if}
                            </div>
                            {#if providerLinkOpen}
                              <div class="review-link-compose">
                                <Link2 size={13} />
                                <input
                                  type="url"
                                  placeholder="Paste an issue, PR, or ticket URL"
                                  bind:value={providerLink}
                                  onkeydown={(event) => {
                                    if (event.key === "Enter") {
                                      event.preventDefault();
                                      void addProviderLink();
                                    }
                                  }}
                                />
                                <button type="button" disabled={!providerLink.trim() || busy} onclick={() => void addProviderLink()}>Add</button>
                              </div>
                            {/if}
                            {#if providerOpen}
                              <div class="review-feedback">
                                {#if providerComments.length === 0}
                                  <p>No review feedback yet.</p>
                                {/if}
                                {#each providerComments as comment (comment.id)}
                                  <article>
                                    <span>{comment.author}</span>
                                    <p>{comment.body}</p>
                                    <button type="button" disabled={busy} onclick={() => void createFollowUp(comment)}>Create follow-up</button>
                                  </article>
                                {/each}
                              </div>
                            {/if}
                          {/if}
                        </div>
                      {/if}

                      {#if worldInsight?.code_avec}
                        <div class="review-context-row">
                          <span class="review-context-icon"><ShieldCheck size={14} strokeWidth={1.7} /></span>
                          <div class="review-context-copy">
                            <p>Code understanding</p>
                            <span>
                              {worldInsight.code_avec.fully_scored_entities} of
                              {worldInsight.code_avec.scoreable_entities} known elements fully understood
                            </span>
                          </div>
                        </div>
                      {/if}

                      {#if patch || (commands && commands.lines.length)}
                        <div class="review-context-row review-context-row--stack">
                          <button
                            type="button"
                            class="review-context-row-button"
                            aria-expanded={reviewAuditOpen}
                            onclick={() => (reviewAuditOpen = !reviewAuditOpen)}
                          >
                            <span class="review-context-icon"><ShieldCheck size={14} strokeWidth={1.7} /></span>
                            <span class="review-context-copy">
                              <span class="review-context-copy-title">Audit trail</span>
                              <span>Exact patch and command record</span>
                            </span>
                            <ChevronRight
                              size={13}
                              class="review-context-row-chevron {reviewAuditOpen ? 'review-context-row-chevron--open' : ''}"
                            />
                          </button>
                          {#if reviewAuditOpen}
                            <div class="review-audit">
                              <p class="review-audit-revision">
                                {review.baseline_oid?.slice(0, 10)}… → {review.sealed_head_oid?.slice(0, 10)}…
                                {#if review.evidence_digest} · record {review.evidence_digest.slice(0, 16)}…{/if}
                              </p>
                              {#if patch}
                                <pre>{patch.lines.join("\n")}</pre>
                                {#if patch.truncated}
                                  <button type="button" disabled={busy} onclick={() => void loadMorePatch()}>Show more changes · {patch.lines.length} of {patch.total_lines} lines</button>
                                {/if}
                              {/if}
                              {#if commands && commands.lines.length}
                                <p class="review-audit-label">Commands</p>
                                <pre>{commands.lines.join("\n")}</pre>
                                {#if commands.truncated}
                                  <button type="button" disabled={busy} onclick={() => void loadMoreCommands()}>Show more commands · {commands.lines.length} of {commands.total_lines}</button>
                                {/if}
                              {/if}
                            </div>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  {/if}
                </section>

                {#if exportedDestination}
                  <p class="review-exported">
                    Project record saved at <span>{exportedDestination}</span>
                  </p>
                {/if}
              </div>
            </div>

            {#if actions?.review.allowed || actions?.apply.allowed}
              <footer class="review-decision">
                {#if reviewNoteOpen && actions?.review.allowed}
                  <label class="sr-only" for="review-rationale">Review note</label>
                  <textarea
                    id="review-rationale"
                    rows="2"
                    class="review-note"
                    placeholder="Anything the next person should know?"
                    bind:value={reviewRationale}
                  ></textarea>
                {/if}
                <div class="review-decision-row">
                  <div class="review-decision-guidance">
                    {#if review.synthesis.unresolved_issues.length}
                      <CircleAlert size={13} strokeWidth={1.7} />
                      <p title={review.synthesis.unresolved_issues.join(" · ")}>
                        {reviewIssueLabel(review.synthesis.unresolved_issues[0])}{review.synthesis.unresolved_issues.length > 1 ? ` · ${review.synthesis.unresolved_issues.length - 1} more` : ""}
                      </p>
                    {:else if actions?.apply.allowed}
                      <Check size={13} strokeWidth={1.8} />
                      <p>Approved and ready to finish</p>
                    {:else}
                      <p>Review the changes, then record your decision.</p>
                    {/if}
                  </div>
                  <div class="review-decision-actions">
                    {#if actions?.review.allowed}
                      <button
                        type="button"
                        class="review-decision-btn"
                        aria-pressed={reviewNoteOpen}
                        onclick={() => (reviewNoteOpen = !reviewNoteOpen)}
                      ><MessageSquarePlus size={13} /><span>{reviewRationale.trim() ? "Edit note" : "Add note"}</span></button>
                      <button
                        type="button"
                        class="review-decision-btn review-decision-btn--primary"
                        disabled={busy || (!!review.policy?.violations.length && !acknowledgePolicy)}
                        onclick={() => void recordApproval()}
                      ><Check size={14} /><span>Approve changes</span></button>
                    {:else if actions?.apply.allowed}
                      <button
                        type="button"
                        class="review-decision-btn review-decision-btn--primary"
                        disabled={busy}
                        onclick={() => void applyApproval()}
                      ><Check size={14} /><span>Finish project…</span></button>
                    {/if}
                  </div>
                </div>
              </footer>
            {/if}
          </div>
        {:else if reviewCanvas}
          <div class="flex min-h-0 flex-1 items-center justify-center p-8 text-center">
            <div class="max-w-sm">
              <p class="text-sm font-medium text-surface-200">Nothing to review yet</p>
              <p class="mt-1 text-xs leading-relaxed text-surface-500">
                Keep working in a source tab. Review will become available when the project has a sealed change set.
              </p>
            </div>
          </div>
        {/if}

        {#if exportOpen}
          <div class="review-export-overlay">
            <button
              type="button"
              class="review-export-backdrop"
              aria-label="Close save project record"
              onclick={() => (exportOpen = false)}
            ></button>
            <div
              class="review-export-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="review-export-title"
              tabindex="-1"
            >
              <span class="review-export-icon"><Save size={17} strokeWidth={1.7} /></span>
              <div>
                <h4 id="review-export-title">Save a project record</h4>
                <p>
                  Keep a portable copy of the changes, activity, and decisions from this project.
                </p>
              </div>
              {#if !isCoLocatedWorkshop()}
                <label for="export-destination">Folder on the connected computer</label>
                <input
                  id="export-destination"
                  placeholder="/path/to/project-record"
                  bind:value={exportDestination}
                />
                <small>Files stay on the connected computer.</small>
              {:else}
                <p class="review-export-path">{exportDestination}</p>
              {/if}
              <div class="review-export-actions">
                <button type="button" onclick={() => (exportOpen = false)}>Cancel</button>
                <button
                  type="button"
                  class="review-export-save"
                  disabled={busy || !exportDestination.trim()}
                  onclick={() => void confirmExport()}
                >Save copy</button>
              </div>
            </div>
          </div>
        {/if}

        {#if worldMode}
          <div
            bind:this={worldEl}
            class="absolute inset-y-0 right-0 z-30 w-[min(32rem,100%)] overflow-auto border-l border-surface-500/40 bg-surface-950/98 p-3 shadow-2xl"
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h4 class="text-sm font-semibold">Understand this code</h4>
                <p class="text-[10px] text-surface-500">See relationships and possible impact without leaving your work</p>
              </div>
              <div class="flex items-center gap-1 text-[10px]">
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
                <button
                  type="button"
                  class="ml-1 rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200"
                  aria-label="Close code understanding"
                  title="Close"
                  onclick={() => (worldMode = false)}
                >×</button>
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

  .review-canvas {
    width: 100%;
    max-width: 88rem;
    margin: 0 auto;
  }

  .review-policy {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    margin-top: 0.85rem;
    border: 1px solid rgb(var(--color-warning-500) / 0.26);
    border-radius: 0.6rem;
    padding: 0.75rem;
    background: rgb(var(--color-warning-950) / 0.12);
    color: rgb(var(--color-warning-300));
  }

  .review-policy p {
    font-size: 0.6875rem;
    font-weight: 600;
  }

  .review-policy ul {
    margin-top: 0.3rem;
    font-size: 0.625rem;
    line-height: 1.5;
    color: rgb(var(--color-warning-200) / 0.72);
  }

  .review-policy li span {
    font-family: var(--font-mono);
  }

  .review-policy label {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    margin-top: 0.55rem;
    font-size: 0.625rem;
    color: rgb(var(--color-warning-100));
  }

  .review-context {
    margin-top: 0.85rem;
    padding: 0.2rem 0 0.8rem;
  }

  .review-context-disclosure {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.3rem 0.4rem 0.3rem 0.1rem;
    color: rgb(var(--color-surface-500));
    text-align: left;
    transition: color 140ms ease, background-color 140ms ease;
  }

  .review-context-disclosure:hover {
    background: rgb(var(--color-surface-800) / 0.3);
    color: rgb(var(--color-surface-200));
  }

  .review-context-disclosure > span {
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .review-context-disclosure small {
    margin-left: 0.25rem;
    font-size: 0.625rem;
    font-weight: 400;
    color: rgb(var(--color-surface-600));
  }

  :global(.review-context-chevron),
  :global(.review-context-row-chevron) {
    flex-shrink: 0;
    transition: transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  :global(.review-context-chevron--open),
  :global(.review-context-row-chevron--open) {
    transform: rotate(90deg);
  }

  .review-context-body {
    margin-top: 0.35rem;
    animation: review-context-in 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  @keyframes review-context-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .review-context-loading {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.55rem;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-500));
  }

  .review-context-loading-dot {
    width: 0.35rem;
    height: 0.35rem;
    border-radius: 50%;
    background: rgb(var(--color-primary-400));
    animation: review-context-pulse 1.2s ease-in-out infinite;
  }

  @keyframes review-context-pulse {
    50% {
      opacity: 0.35;
      transform: scale(0.75);
    }
  }

  .review-context-row,
  .review-context-row-main,
  .review-context-row-button {
    display: grid;
    grid-template-columns: 1.75rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
  }

  .review-context-row {
    min-height: 2.8rem;
    border-radius: 0.5rem;
    padding: 0.45rem 0.55rem;
    transition: background-color 140ms ease;
  }

  .review-context-row:hover {
    background: rgb(var(--color-surface-800) / 0.2);
  }

  .review-context-row--stack {
    display: block;
  }

  .review-context-row-button {
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    text-align: left;
  }

  .review-context-icon {
    display: inline-flex;
    width: 1.75rem;
    height: 1.75rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--color-surface-500));
  }

  .review-context-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
  }

  .review-context-copy p,
  .review-context-copy-title {
    overflow: hidden;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-context-copy > span:not(.review-context-copy-title) {
    overflow: hidden;
    margin-top: 0.1rem;
    font-size: 0.625rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-500));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-context-action {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.35rem 0.5rem;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-500));
  }

  .review-context-action:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .review-context-action:disabled {
    opacity: 0.35;
  }

  .review-timeline {
    margin: 0.5rem 0 0.25rem 2.65rem;
    padding-left: 0.75rem;
    border-left: 1px solid rgb(var(--color-surface-500) / 0.2);
  }

  .review-timeline li {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 0.35rem 0;
  }

  .review-timeline-dot {
    position: absolute;
    top: 0.68rem;
    left: -0.95rem;
    width: 0.35rem;
    height: 0.35rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.45);
    border-radius: 50%;
    background: rgb(var(--color-surface-900));
  }

  .review-timeline p {
    font-size: 0.625rem;
    color: rgb(var(--color-surface-300));
  }

  .review-timeline span,
  .review-timeline time {
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
  }

  .review-context-subactions,
  .review-linked-items {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    margin: 0.35rem 0 0 2.3rem;
  }

  .review-context-subactions button,
  .review-linked-items button,
  .review-audit button {
    border: 0;
    background: transparent;
    padding: 0.15rem 0.3rem;
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-500));
  }

  .review-context-subactions button:hover,
  .review-linked-items button:hover,
  .review-audit button:hover {
    color: rgb(var(--color-primary-300));
  }

  .review-linked-items button {
    max-width: 24rem;
    overflow: hidden;
    border-radius: 0.3rem;
    background: rgb(var(--color-surface-800) / 0.35);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-link-compose {
    display: flex;
    max-width: 34rem;
    align-items: center;
    gap: 0.4rem;
    margin: 0.45rem 0 0 2.3rem;
    border-radius: 0.45rem;
    padding: 0.3rem 0.4rem;
    background: rgb(var(--color-surface-800) / 0.28);
    color: rgb(var(--color-surface-500));
  }

  .review-link-compose input {
    appearance: none;
    min-width: 0;
    flex: 1;
    border: 0;
    background: transparent;
    padding: 0.15rem;
    box-shadow: none;
    outline: none;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-100));
  }

  .review-link-compose input::placeholder {
    color: rgb(var(--color-surface-600));
  }

  .review-link-compose button {
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.25rem 0.4rem;
    font-size: 0.625rem;
    color: rgb(var(--color-primary-300));
  }

  .review-link-compose button:disabled {
    color: rgb(var(--color-surface-600));
  }

  .review-feedback {
    margin: 0.55rem 0 0 2.3rem;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.18);
    border-radius: 0.45rem;
  }

  .review-feedback > p,
  .review-feedback article {
    padding: 0.6rem;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-500));
  }

  .review-feedback article + article {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.15);
  }

  .review-feedback article span {
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
  }

  .review-feedback article p {
    margin-top: 0.2rem;
    white-space: pre-wrap;
    color: rgb(var(--color-surface-300));
  }

  .review-feedback article button {
    margin-top: 0.35rem;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.5625rem;
    color: rgb(var(--color-primary-300));
  }

  .review-audit {
    margin: 0.55rem 0 0 2.3rem;
  }

  .review-audit-revision,
  .review-audit-label {
    margin-bottom: 0.35rem;
    font-family: var(--font-mono);
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
  }

  .review-audit-label {
    margin-top: 0.65rem;
    font-family: inherit;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .review-audit pre {
    max-height: 12rem;
    overflow: auto;
    border-radius: 0.45rem;
    padding: 0.6rem;
    background: rgb(0 0 0 / 0.24);
    font-family: var(--font-mono);
    font-size: 0.5625rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-400));
  }

  .review-exported {
    margin: 0.4rem 0;
    font-size: 0.625rem;
    color: rgb(var(--color-success-300));
  }

  .review-exported span {
    font-family: var(--font-mono);
  }

  .review-decision {
    flex-shrink: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
    padding: 0.6rem 1rem;
    background: rgb(var(--color-surface-900) / 0.94);
    box-shadow: 0 -0.5rem 1.5rem rgb(0 0 0 / 0.14);
    backdrop-filter: blur(12px);
  }

  .review-note {
    appearance: none;
    width: 100%;
    margin-bottom: 0.55rem;
    resize: none;
    border: 0;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-800) / 0.4);
    padding: 0.55rem 0.65rem;
    box-shadow: none;
    outline: none;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
  }

  .review-note::placeholder {
    color: rgb(var(--color-surface-600));
  }

  .review-decision-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .review-decision-guidance {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.45rem;
    color: rgb(var(--color-warning-300));
  }

  .review-decision-guidance p {
    overflow: hidden;
    font-size: 0.625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-decision-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
  }

  .review-decision-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid transparent;
    border-radius: 0.45rem;
    background: transparent;
    padding: 0.4rem 0.6rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }

  .review-decision-btn:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .review-decision-btn--primary {
    border-color: rgb(var(--color-primary-500) / 0.28);
    background: rgb(var(--color-primary-500) / 0.1);
    color: rgb(var(--color-primary-200));
  }

  .review-decision-btn--primary:hover:not(:disabled) {
    background: rgb(var(--color-primary-500) / 0.18);
    color: rgb(var(--color-primary-100));
  }

  .review-decision-btn:disabled {
    opacity: 0.35;
  }

  .review-export-overlay {
    position: absolute;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
  }

  .review-export-backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    background: rgb(0 0 0 / 0.5);
    backdrop-filter: blur(3px);
  }

  .review-export-dialog {
    position: relative;
    display: grid;
    width: min(26rem, 100%);
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.7rem 0.8rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.8rem;
    padding: 1rem;
    background: rgb(var(--color-surface-900) / 0.98);
    box-shadow: 0 1.5rem 4rem rgb(0 0 0 / 0.4);
  }

  .review-export-icon {
    display: inline-flex;
    width: 2rem;
    height: 2rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    background: rgb(var(--color-primary-500) / 0.12);
    color: rgb(var(--color-primary-300));
  }

  .review-export-dialog h4 {
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .review-export-dialog p {
    margin-top: 0.2rem;
    font-size: 0.6875rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-500));
  }

  .review-export-dialog label,
  .review-export-dialog input,
  .review-export-dialog small,
  .review-export-path,
  .review-export-actions {
    grid-column: 1 / -1;
  }

  .review-export-dialog label {
    margin-top: 0.35rem;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-400));
  }

  .review-export-dialog input {
    appearance: none;
    border: 0;
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.5);
    padding: 0.5rem 0.6rem;
    box-shadow: none;
    outline: none;
    font-family: var(--font-mono);
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-100));
  }

  .review-export-dialog input::placeholder {
    color: rgb(var(--color-surface-600));
  }

  .review-export-dialog small {
    margin-top: -0.45rem;
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
  }

  .review-export-dialog .review-export-path {
    overflow: hidden;
    margin-top: 0.35rem;
    border-radius: 0.45rem;
    padding: 0.5rem 0.6rem;
    background: rgb(var(--color-surface-800) / 0.35);
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: rgb(var(--color-surface-400));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-export-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.35rem;
    margin-top: 0.35rem;
  }

  .review-export-actions button {
    border: 1px solid transparent;
    border-radius: 0.4rem;
    background: transparent;
    padding: 0.4rem 0.6rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }

  .review-export-actions button:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .review-export-actions .review-export-save {
    border-color: rgb(var(--color-primary-500) / 0.3);
    background: rgb(var(--color-primary-500) / 0.12);
    color: rgb(var(--color-primary-200));
  }

  .review-export-actions button:disabled {
    opacity: 0.35;
  }

  @media (max-width: 640px) {
    .review-context-disclosure small {
      display: none;
    }

    .review-context-row,
    .review-context-row-main,
    .review-context-row-button {
      grid-template-columns: 1.75rem minmax(0, 1fr);
    }

    .review-context-action,
    :global(.review-context-row-chevron) {
      grid-column: 2;
      justify-self: flex-start;
    }

    .review-decision-row {
      align-items: flex-start;
      flex-direction: column;
    }

    .review-decision-actions {
      align-self: stretch;
      justify-content: flex-end;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .review-context-body,
    :global(.review-context-chevron),
    :global(.review-context-row-chevron),
    .review-context-loading-dot {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
