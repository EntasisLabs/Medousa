<script lang="ts">
  import { EXTERNAL_AGENTS, isExternalRuntime, externalAgent } from "$lib/utils/externalAgentCatalog";
  import type { ExternalAgentRuntime } from "$lib/utils/externalAgentCatalog";

  import { hostedReviews } from "$lib/code/hostedReviews.svelte";
  import { onMount, onDestroy, untrack } from "svelte";
  import { createReviewPage } from "$lib/code/reviewPage.svelte";
  import { reviewRepository } from "$lib/code/reviewQueue";
  import {
    MessageSquare,
    GitPullRequestArrow,
    MoreHorizontal,
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
  import UndertakingReviewActions from "$lib/components/work/UndertakingReviewActions.svelte";
  import type { WorldLocationIntent } from "$lib/work/undertakingWorldController";
  import {
    closeUndertaking,
    interruptTrackedAgent,
    landCodeWorkingSet,
    openProjectTerminal,
    reclaimTrackedHuman,
    startTrackedAgent,
    undertakingWorkspaceCopy,
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
    reviewScope?: string;
    /** Detached Code windows keep file controls in the project menu. */
    filesVisible?: boolean;
    onToggleFiles?: () => void;
  }

  let {
    showBrowser = true,
    workId: boundWorkId = "",
    resource = { kind: "workspace" },
    interactive = true,
    reviewScope = "local",
    filesVisible = true,
    onToggleFiles,
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
  let integrationStrategy = $state<"fast_forward_only" | "preserve_branch">("fast_forward_only");
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
  let preferredCodeAgent = $state<ExternalAgentRuntime>("codex");
  let providerHandoff = $state<ProviderHandoff | null>(null);
  let providerComments = $state<ProviderComment[]>([]);
  let providerLink = $state("");
  let providerOpen = $state(false);
  let reviewDetailsFor = $state<string | null>(null);
  let reviewDetailsLoading = $state(false);
  let reviewDetailsOpen = $state(false);
  let reviewHistoryExpanded = $state(false);
  let reviewHistoryOpen = $state(false);
  let reviewLinksOpen = $state(false);
  let reviewAuditOpen = $state(true);
  let providerLinkOpen = $state(false);
  /** When threads exist, `c` can dismiss the rail without losing comments. */
  let commentRailDismissed = $state(false);

  const reviewCanvas = $derived(resource.kind === "review");
  // The host keys this component by workshop, project, and resource kind.
  const reviewPage = untrack(() => createReviewPage(boundWorkId, reviewScope));
  onDestroy(() => reviewPage.dispose());
  const detail = $derived(reviewCanvas ? reviewPage.detail : (
    !boundWorkId || undertakings.detail?.id === boundWorkId ? undertakings.detail : null
  ));
  const review = $derived(reviewCanvas ? reviewPage.review : (
    !boundWorkId || undertakings.review?.work_id === boundWorkId ? undertakings.review : null
  ));
  $effect(() => {
    if (!reviewCanvas || !boundWorkId) return;
    void undertakings.eventRevision;
    void untrack(() => reviewPage.refresh());
  });
  async function refreshSelectedDetail() {
    if (reviewCanvas) await reviewPage.refresh();
    else await undertakings.refreshDetail();
  }
  function selectReviewAttempt(attemptId: string) {
    return reviewCanvas ? reviewPage.selectCandidate(attemptId) : undertakings.selectReviewAttempt(attemptId);
  }
  const resourcePath = $derived(
    resource.kind === "file" ? resource.path : null,
  );
  const actions = $derived(reviewCanvas ? review?.allowed_actions ?? detail?.allowed_actions : detail?.allowed_actions);
  const workspaceCopy = $derived(undertakingWorkspaceCopy(detail));
  const sourceBranch = $derived(gitTargetBaseRef(detail?.target) ?? "source branch"),
    reviewedBranch = $derived(detail?.environment?.branch?.trim() || "working branch");
  const recordedIntegrationStrategy = $derived(review?.decision?.strategy ?? detail?.review_decisions?.at(-1)?.strategy ?? null);
  const integrationActionLabel = $derived.by(() => {
    const strategy = recordedIntegrationStrategy ?? integrationStrategy;
    if (workspaceCopy.attached || strategy === "keep_checkout") return "Keep changes here";
    return strategy === "fast_forward_only" ? `Apply to ${sourceBranch}` : `Keep ${reviewedBranch}`;
  });
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
    if (isExternalRuntime(savedAgent)) {
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
      if ((err as { status?: number }).status === 409 && (boundWorkId || undertakings.selectedId)) {
        await refreshSelectedDetail();
      }
    } finally {
      busy = false;
    }
  }

  async function openTerminalTracked() {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await openProjectTerminal(d);
      await refreshSelectedDetail();
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

  async function startAgent(runtime: ExternalAgentRuntime) {
    const d = detail;
    if (!d) return;
    preferredCodeAgent = runtime;
    localStorage.setItem("medousa-code-agent-runtime", runtime);
    await run(async () => {
      await startTrackedAgent(d, runtime);
      await refreshSelectedDetail();
    });
  }

  async function handoffToAgent(runtime: ExternalAgentRuntime, draft?: string) {
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
      await refreshSelectedDetail();
    });
  }

  async function reclaimHuman() {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await reclaimTrackedHuman(d);
      await refreshSelectedDetail();
    });
  }

  async function interruptAgent() {
    const d = detail;
    if (!d) return;
    await run(async () => {
      await interruptTrackedAgent(d);
      await refreshSelectedDetail();
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
    isExternalRuntime(undertakings.active?.executorKind) ? externalAgent(undertakings.active!.executorKind as ExternalAgentRuntime).label : "Agent",
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
        await refreshSelectedDetail();
      } catch (err) {
        actionError = humanizeForgeMessage(
          err instanceof Error ? err.message : String(err),
        );
        return;
      }
    }
    if (!leaseId || generation == null) {
      actionError = "Nothing to prepare yet — make a change first, then choose Prepare merge.";
      return;
    }
    await run(async () => {
      await sealLease(leaseId!, generation!);
      await refreshSelectedDetail();
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

  const reviewEvidenceId = $derived(review?.evidence_id);
  $effect(() => {
    void reviewEvidenceId;
    commentRailDismissed = true;
    reviewHistoryExpanded = false;
    reviewHistoryOpen = false;
    reviewLinksOpen = false;
    acknowledgePolicy = false;
    acknowledgeBlocking = false;
    commentCompose = null;
    commentDraft = "";
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
        && !busy && !reviewPage.loading
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
    const strategy = workspaceCopy.attached ? "keep_checkout" : integrationStrategy;
    await run(async () => {
      await recordReviewIntent(detail.id, {
        evidence_id: review.evidence_id!,
        evidence_digest: review.evidence_digest!,
        strategy,
        rationale: reviewRationale.trim() || "Reviewed in Medousa",
        acknowledged_violations: acknowledgePolicy
          ? (review.policy?.violations.map((violation) => violation.id) ?? [])
          : [],
      });
      await refreshSelectedDetail();
      toast.show(
        strategy === "fast_forward_only"
          ? `Approved — ready to apply to ${sourceBranch}.`
          : strategy === "preserve_branch"
            ? `Approved — ${reviewedBranch} will be kept separate.`
            : "Approved — changes will stay in this checkout.",
      );
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
      await refreshSelectedDetail();
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
      await refreshSelectedDetail();
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
    Boolean(commentCompose) || !commentRailDismissed,
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
      await refreshSelectedDetail();
    });
  }

  async function resolveComment(commentId: string) {
    if (!detail) return;
    await run(async () => {
      await resolveReviewComment(detail!.id, commentId);
      await refreshSelectedDetail();
    });
  }

  async function removeComment(commentId: string) {
    if (!detail) return;
    await run(async () => {
      await deleteReviewComment(detail!.id, commentId);
      await refreshSelectedDetail();
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
      await refreshSelectedDetail();
      await lmeWorkspace.openCodeFile(detail!.id, result.path, { line: 1 });
      undertakings.setSelection({ path: result.path, line: 1, entityId: null });
    });
  }

  async function applyApproval() {
    if (!detail) return;
    const decisionId = review?.decision?.id ?? detail.review_decisions?.at(-1)?.id;
    if (!decisionId) return;
    const strategy = recordedIntegrationStrategy;
    const prompt = workspaceCopy.attached || strategy === "keep_checkout"
      ? "Finish this project and keep the changes in this checkout?"
      : strategy === "fast_forward_only"
        ? `Apply the reviewed changes to ${sourceBranch}? This only succeeds if ${sourceBranch} can be fast-forwarded safely.`
        : `Finish this project and keep ${reviewedBranch} as a separate branch? ${sourceBranch} will not change.`;
    if (!window.confirm(prompt)) return;
    await run(async () => {
      const result = await applyDecision(detail!.id, decisionId);
      await refreshSelectedDetail();
      await undertakings.refreshList();
      toast.show(
        result.disposition === "base_fast_forwarded"
          ? `Applied reviewed changes to ${sourceBranch}.`
          : result.disposition === "branch_preserved"
            ? `Finished — ${reviewedBranch} was kept separate.`
            : "Finished — changes remain in this checkout.",
      );
    });
  }

  async function shareProject() {
    if (!detail) return;
    await run(async () => {
      providerHandoff = await shareProviderHandoff(detail!.id, {
        connection_id: hostedReviews.connectionId ?? undefined,
        provider_id: hostedReviews.accountProvider ?? undefined,
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
    if (!window.confirm(workspaceCopy.closePrompt)) return;
    await run(async () => {
      await closeUndertaking(detail!);
      undertakings.clearActive();
      await undertakings.refreshList();
      await undertakings.select("");
    });
  }

</script>

<div class="undertaking-panel flex h-full min-h-0 flex-col text-sm text-surface-100" class:focused-review={reviewCanvas} class:project-panel={!reviewCanvas && showBrowser}>
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

  {#if actionError || (reviewCanvas ? reviewPage.error : undertakings.error)}
    <p class="rounded-md border border-amber-500/40 bg-amber-950/40 px-2 py-1 text-xs text-amber-100">
      {humanizeForgeMessage(actionError || (reviewCanvas ? reviewPage.error : undertakings.error) || "")}
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
            <p class="text-sm font-medium text-content-secondary">{reviewCanvas ? (reviewPage.loading ? "Loading review…" : "Could not open this review") : "Open or start a project"}</p>
            {#if reviewCanvas}<button type="button" class="mt-3 text-xs text-content-secondary" onclick={() => void reviewPage.refresh()}>Try again</button>
            {:else}<p class="mt-1 text-xs leading-relaxed text-content-quiet">Pick a project in the rail, or start one with the outcome you want.</p>{/if}
          </div>
        </div>
      {:else}
        {#if reviewCanvas || !detail.environment}
        <div class="review-page-header flex shrink-0 flex-wrap items-center justify-between gap-2 {showBrowser ? 'px-0' : 'border-b border-surface-500/25 px-2 py-1'}">
          <div class="min-w-0 flex-1">
            {#if reviewCanvas}<p class="review-page-eyebrow"><GitPullRequestArrow size={13} /><span>Forge</span><span>{reviewRepository(detail)}</span><span>{humanPhaseLabel(detail.human_phase)}</span></p>
            {:else}<h3 class="review-page-title truncate text-sm font-semibold text-surface-50" title={detail.title}>{detail.title}</h3>{/if}
            {#if detail.environment && !reviewCanvas}
              <p class="mt-0.5 text-[10px] text-content-quiet">
                {humanPhaseLabel(detail.human_phase)}{workspaceCopy.attached ? " · Current checkout" : ""}
              </p>
            {:else if !reviewCanvas}
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
            {#if reviewCanvas && review}
              <button type="button" class="review-notes-action" aria-label="Review notes" aria-expanded={showCommentRail} onclick={toggleCommentRail}><MessageSquare size={14} /><span>Notes</span></button>
              <UndertakingReviewActions
                attached={workspaceCopy.attached} busy={busy || reviewPage.loading} canApprove={canApproveReview}
                allowReview={Boolean(actions?.review.allowed)} allowApply={Boolean(actions?.apply.allowed)}
                {sourceBranch} {reviewedBranch} actionLabel={integrationActionLabel}
                bind:integrationStrategy
                onRequestChanges={() => void beginRequestChanges()}
                onApprove={() => void recordApproval()}
                onApply={() => void applyApproval()}
              />
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


            <OverflowMenu
              label="Project actions"
              title="Project actions"
              panelClass="review-project-menu w-[min(22rem,calc(100vw-2rem))] rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
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
              {#if reviewCanvas && review}
                <button type="button" role="menuitem" class="secondary-action" onclick={() => void lmeWorkspace.openCodeWorkspace(detail.id, detail.title)}>Open project</button>
                <button type="button" role="menuitem" class="secondary-action" aria-expanded={reviewHistoryOpen} onclick={() => reviewHistoryOpen = !reviewHistoryOpen}>History <span class="review-menu-count">{review.timeline.length}</span></button>
                {#if reviewHistoryOpen}
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
                {/if}
                <button type="button" role="menuitem" class="secondary-action" aria-expanded={reviewLinksOpen} onclick={() => reviewLinksOpen = !reviewLinksOpen}>Pull request{#if reviewPrLinkCount}<span class="review-menu-count">{reviewPrLinkCount}</span>{/if}</button>
                {#if reviewLinksOpen}
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
                {/if}
                <div class="my-1 border-t border-surface-500/25" role="separator"></div>
              {/if}
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
                <div class="my-1 border-t border-surface-500/25" role="separator"></div>
              {/if}
              {#if actions?.start_agent.allowed}
                {#each EXTERNAL_AGENTS as agent (agent.id)}
                  <button type="button" role="menuitem" class="secondary-action" disabled={busy} title={actions?.start_agent.reason ?? ""} onclick={() => void startAgent(agent.id)}>Ask {agent.label} to continue</button>
                {/each}
              {/if}
              <button
                type="button"
                role="menuitem"
                class="secondary-action"
                disabled={busy || !actions?.open_terminal.allowed}
                onclick={() => void openTerminalTracked()}
              >Terminal in {workspaceCopy.label}</button>
              {#if detail.environment?.worktree}
                <button
                  type="button"
                  role="menuitem"
                  class="secondary-action"
                  disabled={busy}
                  onclick={() => void revealWorktree()}
                >Reveal {workspaceCopy.label}</button>
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
                    {workspaceCopy.attached ? "Current checkout" : "Working copy"}: {detail.environment.worktree}<br />Starting revision:
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
              {#if onToggleFiles}
                <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={onToggleFiles}>{filesVisible ? "Hide files" : "Show files"}</button>
                <div class="code-chrome-menu-sep" role="separator"></div>
              {/if}
              {#if actions?.start_agent.allowed}
                {#each EXTERNAL_AGENTS as agent (agent.id)}
                  <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={busy} title={actions?.start_agent.reason ?? ""} onclick={() => void startAgent(agent.id)}>Ask {agent.label} to continue</button>
                {/each}
              {/if}
              <button
                type="button"
                role="menuitem"
                class="code-chrome-menu-item"
                disabled={busy || !actions?.open_terminal.allowed}
                onclick={() => void openTerminalTracked()}
              >Terminal in {workspaceCopy.label}</button>
              {#if detail.environment?.worktree}
                <button
                  type="button"
                  role="menuitem"
                  class="code-chrome-menu-item"
                  disabled={busy}
                  onclick={() => void revealWorktree()}
                >Reveal {workspaceCopy.label}</button>
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
                    {workspaceCopy.attached ? "Current checkout" : "Working copy"}: {detail.environment.worktree}<br />Starting revision:
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
                {workspaceCopy.setupGuidance}
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
            readingState={reviewPage.reading}
            originalRequest={detail.brief}
            {sourceBranch}
            humanPhase={detail.human_phase}
            projectTitle={detail.title}
            provenanceBaseRef={gitTargetBaseRef(detail?.target) || null}
            busy={busy || reviewPage.loading}
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
            onOpenFile={(path, line) => { void lmeWorkspace.openCodeFile(detail.id, path, { line }); }}
            onRestore={restoreReviewedFile}
            onSelectCandidate={(attemptId) => void selectReviewAttempt(attemptId)}
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
