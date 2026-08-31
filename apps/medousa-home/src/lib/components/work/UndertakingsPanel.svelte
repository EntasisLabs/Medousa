<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    GitPullRequestArrow,
    History,
    MessageSquareWarning,
    MoreHorizontal,
    Pencil,
    Play,
    Square,
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
    getEvidencePatch,
    getEvidenceCommands,
    restoreReviewFile,
    addReviewComment,
    resolveReviewComment,
    deleteReviewComment,
    requestReviewChanges,
    continueEditing,
    canStartHumanEditing,
    startHumanEditingSession,
    exportUndertakingBundle,
    humanPhaseGuidance,
    humanPhaseLabel,
    humanizeForgeMessage,
    gitTargetBaseRef,
    getProviderHandoff,
    shareProviderHandoff,
    saveProviderContext,
    getProviderComments,
    importProviderComment,
    type EvidencePage,
    type ReviewFileDiff,
    type ProviderHandoff,
    type ProviderComment,
  } from "$lib/code/undertakingCommandController";
  import UndertakingWorldPanel from "$lib/components/work/UndertakingWorldPanel.svelte";
  import UndertakingReviewCanvas from "$lib/components/work/UndertakingReviewCanvas.svelte";
  import type { WorldLocationIntent } from "$lib/work/undertakingWorldController";
  import {
    closeUndertaking,
    interruptTrackedAgent,
    landCodeWorkingSet,
    openTrackedTerminal,
    reclaimTrackedHuman,
    startTrackedAgent,
  } from "$lib/utils/undertakingWorkspace";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { vault } from "$lib/stores/vault.svelte";
  import { undertakingLocationDeepLinkUrl } from "$lib/deepLinks";
  import { shareText } from "$lib/share";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { loadCodeSourceEditor } from "$lib/runtime/viewLoaders";
  import { isTauri } from "$lib/window";
  import { toast } from "$lib/runtime/toast.svelte";

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
  let busy = $state(false);
  let actionError = $state<string | null>(null);
  let worldMode = $state(false);
  let worldLocate = $state<WorldLocationIntent | null>(null);
  let creating = $state(false);
  let reviewRationale = $state("");
  let reviewNoteOpen = $state(false);
  let acknowledgePolicy = $state(false);
  let acknowledgeBlocking = $state(false);
  let commentDraft = $state("");
  let commentCompose = $state<{
    path: string;
    side: "new" | "old" | string;
    line: number;
    content: string;
  } | null>(null);
  let requestChangesPreviewOpen = $state(false);
  let exportOpen = $state(false);
  let exportDestination = $state("");
  let exportedDestination = $state<string | null>(null);
  let preferredCodeAgent = $state<"codex" | "cursor" | "hermes">("codex");
  let providerHandoff = $state<ProviderHandoff | null>(null);
  let providerComments = $state<ProviderComment[]>([]);
  let providerLink = $state("");
  let providerOpen = $state(false);
  let reviewDetailsFor = $state<string | null>(null);
  let reviewDetailsLoading = $state(false);
  let reviewDetailsOpen = $state(false);
  let reviewHistoryExpanded = $state(false);
  let reviewAuditOpen = $state(true);
  let providerLinkOpen = $state(false);
  /** When threads exist, `c` can dismiss the rail without losing comments. */
  let commentRailDismissed = $state(false);

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
  onMount(() => {
    // Default repo path only when Home shares the workshop disk.
    if (isCoLocatedWorkshop()) {
      const root = vault.activeVaultRoot;
      if (root?.path) repoPath = root.path;
    }
    const savedAgent = localStorage.getItem("medousa-code-agent-runtime");
    if (savedAgent === "cursor" || savedAgent === "codex" || savedAgent === "hermes") {
      preferredCodeAgent = savedAgent;
    }
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
    if (!worldMode) worldLocate = null;
  }

  async function openReviewFromEditor() {
    if (!detail) return;
    await lmeWorkspace.openCodeReview(detail.id, `Review · ${detail.title}`);
  }

  async function startAgent(runtime: "codex" | "cursor" | "hermes") {
    const d = detail;
    if (!d) return;
    preferredCodeAgent = runtime;
    localStorage.setItem("medousa-code-agent-runtime", runtime);
    await run(async () => {
      await startTrackedAgent(d, runtime);
      await undertakings.refreshDetail();
    });
  }

  async function handoffToAgent(runtime: "codex" | "cursor" | "hermes", draft?: string) {
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
        undertakings.active.executorKind !== "human" &&
        undertakings.active.leaseId &&
        undertakings.active.leaseGeneration != null,
    ),
  );
  const agentLabel = $derived(
    undertakings.active?.executorKind === "cursor"
      ? "Cursor"
      : undertakings.active?.executorKind === "hermes"
        ? "Hermes"
        : "Codex",
  );
  async function doSeal() {
    let leaseId = undertakings.active?.leaseId ?? null;
    let generation = undertakings.active?.leaseGeneration ?? null;
    if ((!leaseId || generation == null) && detail && canStartHumanEditing(detail.allowed_actions)) {
      try {
        const begun = await startHumanEditingSession(detail.id, detail.allowed_actions);
        leaseId = begun.lease.lease_id;
        generation = begun.lease.generation;
        undertakings.setActiveFromItem(begun.item, {
          leaseId,
          leaseGeneration: generation,
          executorKind: "human",
        });
        await undertakings.refreshDetail();
      } catch (err) {
        actionError = humanizeForgeMessage(
          err instanceof Error ? err.message : String(err),
        );
        return;
      }
    }
    if (!leaseId || generation == null) {
      actionError = "Nothing to seal yet — make a change first, then seal for review.";
      return;
    }
    await run(async () => {
      await sealLease(leaseId!, generation!);
      await undertakings.refreshDetail();
      if (undertakings.review?.evidence_id) {
        patch = await getEvidencePatch(undertakings.review.evidence_id, {
          work_id: undertakings.review.work_id,
          limit: 400,
        });
      }
    });
  }

  async function loadReviewDetails() {
    const current = review;
    if (!current?.evidence_id || reviewDetailsFor === current.evidence_id) return;
    const evidenceId = current.evidence_id;
    reviewDetailsFor = evidenceId;
    reviewDetailsLoading = true;
    commands = null;
    providerHandoff = null;
    const [commandsResult, providerResult] = await Promise.allSettled([
      getEvidenceCommands(evidenceId, { work_id: current.work_id, limit: 100 }),
      getProviderHandoff(current.work_id),
    ]);
    if (review?.evidence_id !== evidenceId) return;
    commands = commandsResult.status === "fulfilled" ? commandsResult.value : null;
    providerHandoff = providerResult.status === "fulfilled" ? providerResult.value : null;
    reviewDetailsLoading = false;
  }

  $effect(() => {
    if (review?.evidence_id) {
      void loadReviewDetails();
    }
  });

  $effect(() => {
    // Fresh evidence → show comments again if any exist.
    void review?.evidence_id;
    commentRailDismissed = false;
    reviewHistoryExpanded = false;
  });

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

  function openWorldAt(input: WorldLocationIntent) {
    worldLocate = input;
    worldMode = true;
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
      return "Project checks haven't run";
    }
    if (issue.toLowerCase().includes("no file changes")) {
      return "Nothing to approve";
    }
    return humanizeForgeMessage(issue);
  }

  const reviewBlockingMessages = $derived(
    (review?.synthesis.issues ?? [])
      .filter((issue) => issue.blocks_approval)
      .map((issue) => issue.message)
      .concat(
        !(review?.synthesis.issues?.length)
          ? (review?.synthesis.unresolved_issues ?? []).filter((message) =>
              /did not pass|policy or content|starting branch|no file changes/i.test(message),
            )
          : [],
      ),
  );

  const reviewHardBlockingMessages = $derived(
    reviewBlockingMessages.filter(
      (message) => !/no project check|project checks haven'?t run/i.test(message),
    ),
  );

  /** Soft check nudges live in the status bar — keep the review footer for real issues / notes. */
  const reviewFooterIssues = $derived(
    (review?.synthesis.unresolved_issues ?? []).filter(
      (message) => !/no project check|project checks haven'?t run/i.test(message),
    ),
  );

  const canApproveReview = $derived(
    Boolean(
      review
        && actions?.review.allowed
        && review.changed_files.length > 0
        && !busy
        && !(review.policy?.violations.length && !acknowledgePolicy)
        && !(reviewHardBlockingMessages.length && !acknowledgeBlocking && !acknowledgePolicy),
    ),
  );

  const reviewPrLinkCount = $derived.by(() => {
    if (!providerHandoff) return 0;
    let count = providerHandoff.links.length;
    if (providerHandoff.review_url) count += 1;
    return count;
  });

  const reviewTimelineNewestFirst = $derived(
    review ? [...review.timeline].reverse() : [],
  );

  const reviewTimelineVisible = $derived(
    reviewHistoryExpanded
      ? reviewTimelineNewestFirst
      : reviewTimelineNewestFirst.slice(0, 6),
  );

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

  async function beginContinueEditing() {
    if (!detail?.allowed_actions.continue_editing?.allowed) return;
    await run(async () => {
      const begun = await continueEditing(detail!.id);
      undertakings.setActiveFromItem(begun.item, {
        leaseId: begun.lease.lease_id,
        leaseGeneration: begun.lease.generation,
        executorKind: "human",
      });
      await undertakings.refreshDetail();
      await lmeWorkspace.openCodeWorkspace(detail!.id, detail!.title);
    });
  }

  async function beginRequestChanges() {
    if (!review?.evidence_id || !review.evidence_digest || !detail) return;
    const summary = reviewRationale.trim() || undefined;
    const unresolvedIds = (review.comments ?? [])
      .filter((comment) => !comment.resolved_at)
      .map((comment) => comment.id);
    await run(async () => {
      const item = await requestReviewChanges(detail!.id, {
        evidence_id: review.evidence_id!,
        evidence_digest: review.evidence_digest!,
        summary,
        comment_ids: unresolvedIds.length ? unresolvedIds : undefined,
      });
      undertakings.setActiveFromItem(item);
      const brief =
        review.revision_brief?.trim()
        || summary
        || unresolvedIds.map((id) => {
            const comment = review.comments?.find((entry) => entry.id === id);
            return comment ? `${comment.path}:${comment.start_line}\n${comment.body}` : "";
          }).filter(Boolean).join("\n\n");
      await startTrackedAgent(item, preferredCodeAgent, {
        draft: brief || undefined,
      });
      await undertakings.refreshDetail();
      toast.show("Changes requested — a new attempt is starting with your feedback.");
    });
  }

  function openCommentCompose(input: {
    path: string;
    side: "new" | "old" | string;
    line: number;
    content: string;
  }) {
    commentCompose = input;
    commentDraft = "";
    commentRailDismissed = false;
  }

  function toggleCommentRail() {
    const hasThreads = (review?.comments?.length ?? 0) > 0;
    if (!hasThreads && !commentCompose) {
      // Summon compose affordance hint by pinning an empty rail briefly is avoided —
      // `.` / hover already start compose. Toggle only when threads earn the rail.
      return;
    }
    commentRailDismissed = !commentRailDismissed;
  }

  function openAboutReview() {
    reviewDetailsOpen = true;
    void loadReviewDetails();
    queueMicrotask(() => {
      document.getElementById("review-about")?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }

  const showCommentRail = $derived(
    Boolean(commentCompose)
      || (((review?.comments?.length ?? 0) > 0) && !commentRailDismissed),
  );

  async function submitComment() {
    if (!detail || !review?.evidence_id || !commentCompose || !commentDraft.trim()) return;
    const compose = commentCompose;
    const body = commentDraft.trim();
    await run(async () => {
      await addReviewComment(detail!.id, {
        evidence_id: review.evidence_id!,
        attempt_id: review.attempt_id ?? undefined,
        path: compose.path,
        side: compose.side,
        start_line: compose.line,
        end_line: compose.line,
        anchor_text: compose.content || null,
        body,
      });
      commentCompose = null;
      commentDraft = "";
      commentRailDismissed = false;
      await undertakings.refreshDetail();
    });
  }

  async function resolveComment(commentId: string) {
    if (!detail) return;
    await run(async () => {
      await resolveReviewComment(detail!.id, commentId);
      await undertakings.refreshDetail();
    });
  }

  async function removeComment(commentId: string) {
    if (!detail) return;
    await run(async () => {
      await deleteReviewComment(detail!.id, commentId);
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
    if (
      !window.confirm(
        `Close “${detail.title}”? Its working copy will be removed.`,
      )
    ) {
      return;
    }
    await run(async () => {
      await closeUndertaking(detail!);
      undertakings.clearActive();
      await undertakings.refreshList();
      await undertakings.select("");
    });
  }

</script>

<div class="flex h-full min-h-0 flex-col gap-3 p-3 text-sm text-surface-100">
  {#if showBrowser}<header class="flex flex-wrap items-center justify-between gap-2 border-b border-surface-500/40 pb-2">
    <div>
      <h2 class="text-base font-semibold text-surface-50">Code projects</h2>
      <p class="text-xs text-content-tertiary">
        One goal, its files, and everyone helping
      </p>
    </div>
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        class="rounded-md border border-surface-500/50 px-2 py-1 text-xs text-content-secondary"
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
          <p class="px-0.5 text-[10px] font-medium uppercase tracking-wide text-content-tertiary">
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
        <p class="text-xs text-content-quiet">Loading…</p>
      {:else if undertakings.items.length === 0 && !creating}
        <p class="px-1 py-3 text-xs leading-relaxed text-content-quiet">
          No code projects yet. Start with a repository and the change you want to make.
        </p>
      {/if}
      <ul class="flex flex-col gap-1">
        {#if activeItems.length}
          <li class="px-1 pt-1 text-[10px] uppercase tracking-wide text-content-quiet">
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
                <span class="text-[10px] text-content-tertiary">
                  {humanPhaseLabel(item.human_phase)}
                </span>
              </button>
            </li>
          {/each}
        {/if}
        {#if completedItems.length}
          <li class="px-1 pt-2 text-[10px] uppercase tracking-wide text-content-quiet">
            Finished
          </li>
          {#each completedItems as item (item.id)}
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
                <span class="text-[10px] text-content-tertiary">
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
            <p class="text-sm font-medium text-content-secondary">Open or start a project</p>
            <p class="mt-1 text-xs leading-relaxed text-content-quiet">
              Pick a project in the rail, or start one with the outcome you want.
            </p>
          </div>
        </div>
      {:else}
        {#if reviewCanvas || !detail.environment}
        <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 {showBrowser ? 'px-0' : 'border-b border-surface-500/25 px-2 py-1'}">
          <div class="min-w-0 flex-1">
            <h3 class="truncate text-sm font-semibold text-surface-50" title={detail.brief || detail.title}>{detail.title}</h3>
            {#if reviewCanvas && review}
              <p class="review-chrome-meta" aria-label="Review summary">
                <span class="review-meta-item review-meta-item--files">
                  <span class="review-meta-value tabular-nums">{review.changed_files.length}</span>
                  {review.changed_files.length === 1 ? "file" : "files"}
                </span>
                <span class="review-meta-dot" aria-hidden="true">·</span>
                <span
                  class="review-meta-item review-meta-item--risk review-meta-item--risk-{review.synthesis.risk}"
                  title={review.synthesis.risk_summary}
                >{review.synthesis.risk} risk</span>
                {#if review.attempt_seq != null}
                  <span class="review-meta-dot" aria-hidden="true">·</span>
                  <span class="review-meta-item review-meta-item--attempt">
                    attempt
                    <span class="review-meta-value tabular-nums">{review.attempt_seq}</span>{#if review.candidates.length > 1}<span class="review-meta-quiet tabular-nums">/{review.candidates.length}</span>{/if}
                  </span>
                {/if}
                {#if review.timeline.length > 0}
                  <span class="review-meta-dot" aria-hidden="true">·</span>
                  <span class="review-meta-item review-meta-item--events">
                    <span class="review-meta-value tabular-nums">{review.timeline.length}</span>
                    {review.timeline.length === 1 ? "event" : "events"}
                  </span>
                {/if}
                {#if reviewPrLinkCount > 0}
                  <span class="review-meta-dot" aria-hidden="true">·</span>
                  <span class="review-meta-item review-meta-item--pr">
                    PR <span class="review-meta-value tabular-nums">({reviewPrLinkCount})</span>
                  </span>
                {:else if (review.changed_since_previous?.length ?? 0) > 0}
                  <span class="review-meta-dot" aria-hidden="true">·</span>
                  <span class="review-meta-item review-meta-item--follow">follow-up</span>
                {/if}
              </p>
            {:else if detail.environment}
              <p class="mt-0.5 text-[10px] text-content-quiet">
                {humanPhaseLabel(detail.human_phase)}
              </p>
            {:else}
              {#if detail.brief && detail.brief.trim() !== detail.title.trim()}
                <p class="truncate text-[10px] text-content-quiet" title={detail.brief}>{detail.brief}</p>
              {/if}
              <p class="mt-0.5 text-[10px] text-content-quiet">
                <span class="font-medium text-content-secondary">{humanPhaseLabel(detail.human_phase)}</span>
                · {humanPhaseGuidance(detail.human_phase)}
              </p>
            {/if}
          </div>
          <div class="flex shrink-0 items-center gap-1.5">
            {#if reviewCanvas && review && actions?.review.allowed}
              {#if actions.continue_editing?.allowed}
                <button
                  type="button"
                  class="scripts-workbench-toolbar-btn"
                  disabled={busy}
                  title="Continue editing"
                  aria-label="Continue editing"
                  onclick={() => void beginContinueEditing()}
                ><Pencil size={14} strokeWidth={1.75} /></button>
              {/if}
              <button
                type="button"
                class="scripts-workbench-toolbar-btn"
                disabled={busy}
                title="Request changes"
                aria-label="Request changes"
                onclick={() => void beginRequestChanges()}
              ><MessageSquareWarning size={14} strokeWidth={1.75} /></button>
              <button
                type="button"
                class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
                disabled={!canApproveReview}
                title="Approve changes"
                aria-label="Approve changes"
                onclick={() => void recordApproval()}
              ><Check size={14} strokeWidth={1.75} /></button>
            {:else if reviewCanvas && review && actions?.apply.allowed}
              <button
                type="button"
                class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
                disabled={busy}
                title="Finish project"
                aria-label="Finish project"
                onclick={() => void applyApproval()}
              ><Check size={14} strokeWidth={1.75} /></button>
            {:else if !detail.environment && actions?.provision.allowed}
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

            {#if reviewCanvas && review}
              <OverflowMenu
                label="Review history"
                title="History"
                panelClass="w-[min(22rem,calc(100vw-2rem))] rounded-lg border border-surface-500/40 bg-surface-900 p-2 shadow-xl"
              >
                {#snippet trigger({ open, toggle })}
                  <button
                    type="button"
                    class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
                    title="History"
                    aria-label="History · {review.timeline.length} events"
                    aria-expanded={open}
                    aria-haspopup="menu"
                    onclick={toggle}
                  ><History size={14} strokeWidth={1.75} /></button>
                {/snippet}
                <div class="review-chrome-popover" role="presentation">
                  <p class="review-chrome-popover-title">
                    History
                    <span class="tabular-nums">{review.timeline.length}</span>
                  </p>
                  {#if reviewTimelineNewestFirst.length === 0}
                    <p class="review-chrome-popover-empty">No events yet.</p>
                  {:else}
                    <ol class="review-chrome-timeline">
                      {#each reviewTimelineVisible as event (event.id)}
                        <li>
                          <div>
                            <p>{event.label}</p>
                            <span>{event.actor_label}{event.detail ? ` · ${event.detail}` : ""}</span>
                          </div>
                          <time>{new Date(event.at).toLocaleString()}</time>
                        </li>
                      {/each}
                    </ol>
                    {#if reviewTimelineNewestFirst.length > 6}
                      <button
                        type="button"
                        class="review-chrome-popover-more"
                        onclick={() => (reviewHistoryExpanded = !reviewHistoryExpanded)}
                      >
                        {reviewHistoryExpanded
                          ? "Show less"
                          : `Show earlier · ${reviewTimelineNewestFirst.length - 6} more`}
                      </button>
                    {/if}
                  {/if}
                </div>
              </OverflowMenu>

              <OverflowMenu
                label="Pull request"
                title="Pull request"
                panelClass="w-[min(20rem,calc(100vw-2rem))] rounded-lg border border-surface-500/40 bg-surface-900 p-2 shadow-xl"
              >
                {#snippet trigger({ open, toggle })}
                  <button
                    type="button"
                    class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
                    title={reviewPrLinkCount > 0 ? `Pull request · ${reviewPrLinkCount} links` : "Pull request"}
                    aria-label={reviewPrLinkCount > 0 ? `Pull request · ${reviewPrLinkCount} links` : "Pull request"}
                    aria-expanded={open}
                    aria-haspopup="menu"
                    onclick={toggle}
                  >
                    <GitPullRequestArrow size={14} strokeWidth={1.75} />
                    {#if reviewPrLinkCount > 0}
                      <span class="review-toolbar-badge tabular-nums">{reviewPrLinkCount}</span>
                    {/if}
                  </button>
                {/snippet}
                <div class="review-chrome-popover" role="presentation">
                  <p class="review-chrome-popover-title">Pull request</p>
                  {#if !providerHandoff || (!providerHandoff.available && !providerHandoff.review_url && providerHandoff.links.length === 0)}
                    <p class="review-chrome-popover-empty">Not linked</p>
                  {:else}
                    {#if providerHandoff.repository || providerHandoff.review_url}
                      <p class="review-chrome-popover-line">
                        {providerHandoff.repository || "Ready to share"}
                      </p>
                    {/if}
                    {#if providerHandoff.review_url}
                      <button
                        type="button"
                        role="menuitem"
                        class="secondary-action"
                        onclick={() => window.open(providerHandoff?.review_url ?? "", "_blank", "noopener,noreferrer")}
                      >Open review</button>
                    {:else if providerHandoff.available}
                      <button
                        type="button"
                        role="menuitem"
                        class="secondary-action"
                        disabled={busy}
                        onclick={() => void shareProject()}
                      >Share branch…</button>
                    {/if}
                    {#each providerHandoff.links as link (link)}
                      <button
                        type="button"
                        role="menuitem"
                        class="secondary-action review-chrome-link"
                        onclick={() => window.open(link, "_blank", "noopener,noreferrer")}
                      >{link}</button>
                    {/each}
                    <button
                      type="button"
                      role="menuitem"
                      class="secondary-action"
                      onclick={() => {
                        reviewDetailsOpen = true;
                        providerLinkOpen = true;
                        void loadReviewDetails();
                      }}
                    >Add related link…</button>
                  {/if}
                </div>
              </OverflowMenu>
            {/if}

            <OverflowMenu
              label="Project actions"
              title="Project actions"
              panelClass="w-52 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
            >
              {#snippet trigger({ open, toggle })}
                <button
                  type="button"
                  class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
                  title="Project actions"
                  aria-label="Project actions"
                  aria-expanded={open}
                  aria-haspopup="menu"
                  onclick={toggle}
                >
                  <MoreHorizontal size={15} strokeWidth={1.75} />
                </button>
              {/snippet}
              {#if reviewCanvas && review && actions?.review.allowed}
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  onclick={() => (reviewNoteOpen = !reviewNoteOpen)}
                >{reviewRationale.trim() ? "Edit note" : "Add note"}</button>
                {#if actions.continue_editing?.allowed}
                  <button
                    type="button"
                    role="menuitem"
                    class="secondary-action"
                    disabled={busy}
                    onclick={() => void beginContinueEditing()}
                  >Continue editing</button>
                {/if}
                {#if review.candidates.length > 1}
                  <div class="px-2 py-1.5 text-[10px] text-content-quiet">
                    <label class="flex flex-col gap-1">
                      <span>Compare sealed attempt</span>
                      <select
                        class="rounded border border-surface-500/35 bg-surface-900 px-1.5 py-1 text-[11px] text-content-secondary"
                        value={review.attempt_id ?? ""}
                        disabled={busy}
                        onchange={(event) => undertakings.selectReviewAttempt(event.currentTarget.value)}
                      >
                        {#each review.candidates as candidate (candidate.attempt_id)}
                          <option value={candidate.attempt_id}>
                            Attempt {candidate.attempt_seq} · {candidate.executor}
                          </option>
                        {/each}
                      </select>
                    </label>
                  </div>
                {/if}
                <div class="my-1 border-t border-surface-500/25" role="separator"></div>
              {/if}
              {#if actions?.start_agent.allowed}
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  disabled={busy}
                  title={actions?.start_agent.reason ?? ""}
                  onclick={() => void startAgent("codex")}
                >Ask Codex to continue</button>
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  disabled={busy}
                  onclick={() => void startAgent("cursor")}
                >Ask Cursor to continue</button>
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  disabled={busy}
                  onclick={() => void startAgent("hermes")}
                >Ask Hermes to continue</button>
              {/if}
              <button
                type="button"
                role="menuitem"
                class="secondary-action"
                disabled={busy || !actions?.open_terminal.allowed}
                onclick={() => void openTerminalTracked()}
              >Terminal in working copy</button>
              {#if detail.environment?.worktree}
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  disabled={busy}
                  onclick={() => void revealWorktree()}
                >Reveal working copy</button>
              {/if}
              {#if undertakings.active?.workId === detail.id && undertakings.active.selectedPath}
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  disabled={busy}
                  onclick={() => void copyLocationLink()}
                >Copy location link</button>
              {/if}
              {#if reviewCanvas && review}
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  onclick={() => openAboutReview()}
                >Review details</button>
                {#if (review.comments?.length ?? 0) > 0}
                  <button
                    type="button"
                    role="menuitem"
                    class="secondary-action"
                    onclick={() => {
                      commentRailDismissed = !commentRailDismissed;
                    }}
                  >{showCommentRail ? "Hide comments" : "Show comments"}</button>
                {/if}
              {/if}
              <div class="my-1 border-t border-surface-500/25" role="separator"></div>
              {#if detail.environment}
                <details class="px-2 py-1 text-chrome-xs text-content-quiet">
                  <summary class="cursor-pointer select-none hover:text-content-secondary">Technical details</summary>
                  <p class="mt-1 break-all font-mono leading-relaxed">
                    Working copy: {detail.environment.worktree}<br />Starting revision:
                    {detail.environment.baseline_oid.slice(0, 12)} · internal state: {detail.state}
                  </p>
                </details>
              {/if}
              <button
                type="button"
                role="menuitem"
                class="secondary-action text-rose-200"
                disabled={busy || !actions?.discard.allowed}
                onclick={() => void discardWithConfirmation()}
              >Close project…</button>
            </OverflowMenu>
          </div>
        </div>
        {/if}

        {#if detail.environment && !reviewCanvas}
          {#await loadCodeSourceEditor()}
            <div class="flex min-h-48 flex-1 items-center justify-center p-6 text-sm text-content-quiet">
              Loading editor…
            </div>
          {:then { default: CodeSourceEditor }}
          <CodeSourceEditor
            fill={!showBrowser}
            workId={detail.id}
            {resourcePath}
            {interactive}
            worldOpen={worldMode}
            projectTitle={detail.title}
            phaseLabel={humanPhaseLabel(detail.human_phase)}
            reviewAvailable={Boolean(actions?.seal.allowed) || Boolean(review && (detail.human_phase === "review" || review.evidence_id))}
            terminalAvailable={Boolean(actions?.open_terminal.allowed)}
            agentRunning={agentRunning}
            agentLabel={agentLabel}
            preferredAgent={preferredCodeAgent}
            onToggleWorld={() => void toggleWorldFromEditor()}
            onOpenReview={() => void (actions?.seal.allowed ? doSeal() : openReviewFromEditor())}
            onOpenTerminal={() => void openTerminalTracked()}
            onStopAgent={() => void interruptAgent()}
            onResumeEditing={() => void reclaimHuman()}
            onProvision={async () => {
              await run(async () => {
                await provisionAndOpenProject(detail.id);
              });
            }}
            onHandoffToAgent={handoffToAgent}
            onReclaimHuman={reclaimHuman}
          >
            {#snippet projectMenu()}
              {#if actions?.start_agent.allowed}
                <button
                  type="button"
                  role="menuitem"
                  class="code-chrome-menu-item"
                  disabled={busy}
                  title={actions?.start_agent.reason ?? ""}
                  onclick={() => void startAgent("codex")}
                >Ask Codex to continue</button>
                <button
                  type="button"
                  role="menuitem"
                  class="code-chrome-menu-item"
                  disabled={busy}
                  onclick={() => void startAgent("cursor")}
                >Ask Cursor to continue</button>
                <button
                  type="button"
                  role="menuitem"
                  class="code-chrome-menu-item"
                  disabled={busy}
                  onclick={() => void startAgent("hermes")}
                >Ask Hermes to continue</button>
              {/if}
              <button
                type="button"
                role="menuitem"
                class="code-chrome-menu-item"
                disabled={busy || !actions?.open_terminal.allowed}
                onclick={() => void openTerminalTracked()}
              >Terminal in working copy</button>
              {#if detail.environment?.worktree}
                <button
                  type="button"
                  role="menuitem"
                  class="code-chrome-menu-item"
                  disabled={busy}
                  onclick={() => void revealWorktree()}
                >Reveal working copy</button>
              {/if}
              {#if undertakings.active?.workId === detail.id && undertakings.active.selectedPath}
                <button
                  type="button"
                  role="menuitem"
                  class="code-chrome-menu-item"
                  disabled={busy}
                  onclick={() => void copyLocationLink()}
                >Copy location link</button>
              {/if}
              {#if detail.environment}
                <details class="px-2 py-1 text-[11px] text-content-quiet">
                  <summary class="cursor-pointer select-none hover:text-content-secondary">Technical details</summary>
                  <p class="mt-1 break-all font-mono leading-relaxed text-[10px]">
                    Working copy: {detail.environment.worktree}<br />Starting revision:
                    {detail.environment.baseline_oid.slice(0, 12)} · internal state: {detail.state}
                  </p>
                </details>
              {/if}
              <div class="code-chrome-menu-sep" role="separator"></div>
              <button
                type="button"
                role="menuitem"
                class="code-chrome-menu-item code-chrome-menu-item--warn"
                disabled={busy || !actions?.discard.allowed}
                onclick={() => void discardWithConfirmation()}
              >Close project…</button>
            {/snippet}
          </CodeSourceEditor>
          {/await}
        {:else if !reviewCanvas && actions?.provision.allowed}
          <div class="flex min-h-48 flex-1 items-center justify-center p-6 text-center">
            <div class="max-w-sm">
              <p class="text-xs font-medium text-content-secondary">Set up this project</p>
              <p class="mt-1 text-[10px] leading-relaxed text-content-quiet">
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
            <p class="max-w-sm text-xs text-content-quiet">
              {humanPhaseGuidance(detail.human_phase)}
            </p>
          </div>
        {/if}

        {#if reviewCanvas}
          <UndertakingReviewCanvas
            {review}
            humanPhase={detail.human_phase}
            projectTitle={detail.title}
            provenanceBaseRef={baseRef || gitTargetBaseRef(detail?.target) || null}
            {busy}
            bind:acknowledgePolicy
            bind:acknowledgeBlocking
            bind:requestChangesPreviewOpen
            bind:reviewDetailsOpen
            bind:reviewAuditOpen
            bind:providerLink
            bind:commentDraft
            bind:commentCompose
            bind:reviewRationale
            bind:exportOpen
            bind:exportDestination
            {reviewHardBlockingMessages}
            {reviewFooterIssues}
            issueLabel={reviewIssueLabel}
            {showCommentRail}
            {reviewDetailsLoading}
            {providerLinkOpen}
            {providerHandoff}
            {commands}
            {exportedDestination}
            {reviewNoteOpen}
            reviewAllowed={Boolean(actions?.review.allowed)}
            remoteExport={!isCoLocatedWorkshop()}
            onOpenFile={(path, line) => openWorldAt({ path, line })}
            onRestore={restoreReviewedFile}
            onSelectCandidate={(attemptId) => undertakings.selectReviewAttempt(attemptId)}
            onComment={openCommentCompose}
            onToggleCommentRail={toggleCommentRail}
            onExport={() => void beginExport()}
            onAddProviderLink={() => void addProviderLink()}
            onLoadMoreCommands={() => void loadMoreCommands()}
            onSubmitComment={submitComment}
            onCancelCompose={() => {
              commentCompose = null;
              commentDraft = "";
            }}
            onResolveComment={resolveComment}
            onDeleteComment={removeComment}
            onConfirmExport={() => void confirmExport()}
          />
        {/if}

        {#if worldMode && detail}
          <UndertakingWorldPanel
            workId={detail.id}
            locate={worldLocate}
            onClose={() => {
              worldMode = false;
              worldLocate = null;
            }}
            onError={(message) => (actionError = message)}
          />
        {/if}
      {/if}
    </section>
  </div>
</div>

<style>
  .review-chrome-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.2rem 0.4rem;
    margin-top: 0.15rem;
    min-width: 0;
    font-size: 0.7rem;
    line-height: 1.35;
    letter-spacing: 0.005em;
    color: rgb(var(--theme-text-secondary));
  }

  .review-meta-item {
    display: inline-flex;
    align-items: baseline;
    gap: 0.22rem;
    white-space: nowrap;
  }

  .review-meta-value {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--theme-text));
  }

  .review-meta-quiet {
    font-weight: 500;
    color: rgb(var(--theme-text-quiet));
  }

  .review-meta-dot {
    color: rgb(var(--theme-text-quiet));
    opacity: 0.7;
  }

  .review-meta-item--risk {
    font-weight: 550;
  }

  /* Mix status into readable text — tint, not neon. */
  .review-meta-item--risk-low {
    color: color-mix(
      in srgb,
      rgb(var(--theme-success)) 40%,
      rgb(var(--theme-text-secondary))
    );
  }

  .review-meta-item--risk-attention {
    color: color-mix(
      in srgb,
      rgb(var(--theme-warning)) 48%,
      rgb(var(--theme-text-secondary))
    );
  }

  .review-meta-item--risk-high {
    color: color-mix(
      in srgb,
      rgb(var(--theme-error)) 52%,
      rgb(var(--theme-text-secondary))
    );
  }

  .review-meta-item--attempt {
    color: rgb(var(--theme-text-secondary));
  }

  .review-meta-item--attempt .review-meta-value {
    color: color-mix(
      in srgb,
      rgb(var(--color-primary-400)) 45%,
      rgb(var(--theme-text))
    );
  }

  .review-meta-item--events {
    color: rgb(var(--theme-text-secondary));
  }

  .review-meta-item--pr,
  .review-meta-item--follow {
    font-weight: 550;
    color: color-mix(
      in srgb,
      rgb(var(--theme-link)) 42%,
      rgb(var(--theme-text-secondary))
    );
  }

  .review-meta-item--pr .review-meta-value {
    color: inherit;
    font-weight: 600;
  }

  :global(html.dark) .review-meta-item--risk-low {
    color: color-mix(
      in srgb,
      rgb(var(--theme-success)) 32%,
      rgb(var(--theme-text-secondary))
    );
  }

  :global(html.dark) .review-meta-item--attempt .review-meta-value {
    color: color-mix(
      in srgb,
      rgb(var(--color-primary-300)) 38%,
      rgb(var(--theme-text))
    );
  }

  :global(html.dark) .review-meta-item--pr,
  :global(html.dark) .review-meta-item--follow {
    color: color-mix(
      in srgb,
      rgb(var(--theme-link)) 34%,
      rgb(var(--theme-text-secondary))
    );
  }

  .secondary-action {
    display: block;
    width: 100%;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.4rem 0.5rem;
    text-align: left;
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 400;
    color: rgb(var(--theme-text));
    cursor: pointer;
  }

  .secondary-action:hover:not(:disabled) {
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--theme-text));
  }

  .secondary-action:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .review-toolbar-badge {
    margin-left: 0.1rem;
    min-width: 0.9rem;
    font-size: 0.625rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
  }

  .review-chrome-popover {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    max-height: min(22rem, 60vh);
    overflow: auto;
  }

  .review-chrome-popover-title {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
    margin: 0;
    padding: 0.15rem 0.35rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--theme-text));
  }

  .review-chrome-popover-title span {
    color: rgb(var(--theme-text-quiet));
    font-weight: 500;
  }

  .review-chrome-popover-empty,
  .review-chrome-popover-line {
    margin: 0;
    padding: 0.25rem 0.35rem;
    font-size: 0.75rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-chrome-timeline {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .review-chrome-timeline li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.65rem;
    padding: 0.2rem 0.35rem;
  }

  .review-chrome-timeline p {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--theme-text));
  }

  .review-chrome-timeline span,
  .review-chrome-timeline time {
    font-size: 0.625rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-chrome-timeline time {
    white-space: nowrap;
  }

  .review-chrome-popover-more {
    border: 0;
    background: transparent;
    padding: 0.35rem;
    color: rgb(var(--theme-link));
    font-size: 0.6875rem;
    text-align: left;
    cursor: pointer;
  }

  .review-chrome-link {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

</style>
