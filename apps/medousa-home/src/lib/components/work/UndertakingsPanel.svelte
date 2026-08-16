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
    MessageSquareWarning,
    MoreHorizontal,
    Pencil,
    Play,
    Save,
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
    gitTargetBaseRef,
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
  } from "$lib/code/undertakingCommandController";
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
  import { openUndertakingLocation } from "$lib/utils/undertakingLocation";
  import { undertakingLocationDeepLinkUrl } from "$lib/deepLinks";
  import { shareText } from "$lib/share";
  import ForgeReviewSurface from "$lib/components/work/ForgeReviewSurface.svelte";
  import ReviewCommentRail from "$lib/components/work/ReviewCommentRail.svelte";
  import ReviewProvenanceStrip from "$lib/components/work/ReviewProvenanceStrip.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { loadCodeSourceEditor } from "$lib/runtime/viewLoaders";
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
  let worldEl = $state<HTMLDivElement | null>(null);
  let preferredCodeAgent = $state<"codex" | "cursor">("codex");
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
  function selectedWorldSnapshot(): WorldSnapshotRef | null {
    return worldBinding?.[worldSnapshot] ?? null;
  }

  const worldSlot = $derived(worldBinding?.[worldSnapshot] ?? null);
  const worldSlotState = $derived((worldSlot?.state ?? "").toLowerCase());
  const worldMapIndexing = $derived(
    worldSlotState === "queued" || worldSlotState === "indexing",
  );
  const worldMapFailed = $derived(worldSlotState === "failed");
  const worldMapReady = $derived(
    worldSlotState === "ready" && worldInsight != null && !worldError,
  );

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
        undertakings.active.executorKind !== "human" &&
        undertakings.active.leaseId &&
        undertakings.active.leaseGeneration != null,
    ),
  );
  const agentLabel = $derived(
    undertakings.active?.executorKind === "cursor" ? "Cursor" : "Codex",
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
    commands = null;
    worldInsight = null;
    providerHandoff = null;
    const [commandsResult, worldResult, providerResult] = await Promise.allSettled([
      getEvidenceCommands(evidenceId, { work_id: current.work_id, limit: 100 }),
      getWorldCodeAvec(current.work_id),
      getProviderHandoff(current.work_id),
    ]);
    if (review?.evidence_id !== evidenceId) return;
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

  async function loadWorldOverview() {
    if (!detail) return;
    busy = true;
    worldError = null;
    try {
      worldBinding = await getWorldBinding(detail.id);
      const slot = worldBinding?.[worldSnapshot];
      const state = (slot?.state ?? "").toLowerCase();
      if (state && state !== "ready") {
        worldFiles = null;
        worldInsight = null;
        worldFind = null;
        worldImpact = null;
        if (state === "failed") {
          worldError = slot?.error?.trim() || "Code map indexing failed.";
        }
        return;
      }
      const snapshot = selectedWorldSnapshot();
      try {
        worldFiles = await getWorldFiles(detail.id, undefined, snapshot);
        worldInsight = await getWorldCodeAvec(detail.id, snapshot);
        worldError = null;
      } catch (err) {
        worldFiles = null;
        worldInsight = null;
        worldError = err instanceof Error ? err.message : String(err);
      }
    } catch (err) {
      worldFiles = null;
      worldInsight = null;
      worldError = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function rebuildWorldMap() {
    if (!detail) return;
    await run(async () => {
      await queueWorldIndex(detail!.id, worldSnapshot);
      worldBinding = await getWorldBinding(detail!.id);
    });
    await loadWorldOverview();
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

  function scrollToReviewFile(path: string) {
    const el = document.getElementById(`diff-file-${encodeURIComponent(path)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
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

  $effect(() => {
    if (worldMode && detail?.id) void loadWorldOverview();
  });
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

        {#if reviewCanvas && review && (detail.human_phase === "review" || review.evidence_id)}
          <div class="flex min-h-0 flex-1 flex-col bg-surface-950/20">
            <div class="min-h-0 flex-1 overflow-auto px-4 py-3">
              <div
                class="review-canvas"
                class:review-canvas--with-rail={showCommentRail}
              >
                <div class="review-canvas-main">
                {#if review.policy && (review.policy.violations.length || review.policy.capture_risks.length)}
                  <section class="review-policy" aria-label="Policy exceptions">
                    <CircleAlert size={15} strokeWidth={1.7} aria-hidden="true" />
                    <div class="min-w-0 flex-1">
                      <p>Review exceptions</p>
                      <ul>
                        {#each review.policy.violations as violation (violation.id)}
                          <li>
                            <button
                              type="button"
                              class="review-policy-path"
                              onclick={() => scrollToReviewFile(violation.path)}
                            >{violation.path}</button>
                            — {violation.detail}
                          </li>
                        {/each}
                        {#each review.policy.capture_risks as risk, riskIndex (`${risk.kind}:${"path" in risk ? risk.path : ""}:${riskIndex}`)}
                          <li>
                            {#if risk.kind === "secret_pattern"}
                              Possible secret in
                              <button
                                type="button"
                                class="review-policy-path"
                                onclick={() => scrollToReviewFile(risk.path)}
                              >{risk.path}</button>
                            {:else if risk.kind === "oversize_file"}
                              Large file
                              <button
                                type="button"
                                class="review-policy-path"
                                onclick={() => scrollToReviewFile(risk.path)}
                              >{risk.path}</button>
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

                {#if reviewHardBlockingMessages.length > 0 && !(review.policy?.violations.length)}
                  <section class="review-policy" aria-label="Blocking review issues">
                    <CircleAlert size={15} strokeWidth={1.7} aria-hidden="true" />
                    <div class="min-w-0 flex-1">
                      <p>Needs acknowledgment before approval</p>
                      <ul>
                        {#each reviewHardBlockingMessages as message (message)}
                          <li>{reviewIssueLabel(message)}</li>
                        {/each}
                      </ul>
                      <label>
                        <input type="checkbox" bind:checked={acknowledgeBlocking} />
                        <span>I reviewed these conditions and want to approve anyway.</span>
                      </label>
                    </div>
                  </section>
                {/if}

                <ForgeReviewSurface
                  {review}
                  projectTitle={detail.title}
                  {busy}
                  onOpenFile={(path, line) => revealLocation({ path, line })}
                  onRestore={restoreReviewedFile}
                  onSelectCandidate={(attemptId) => undertakings.selectReviewAttempt(attemptId)}
                  onComment={openCommentCompose}
                  onToggleCommentRail={toggleCommentRail}
                />

                {#if review.decision?.rationale}
                  <p class="review-prior-note">
                    Prior decision note: <span>{review.decision.rationale}</span>
                  </p>
                {/if}

                {#if review.revision_brief || (review.unresolved_comment_count ?? 0) > 0 || (review.comments?.length ?? 0) > 0}
                  <section class="review-revision-brief" aria-label="Revision brief preview">
                    <button
                      type="button"
                      class="review-context-disclosure"
                      aria-expanded={requestChangesPreviewOpen}
                      onclick={() => (requestChangesPreviewOpen = !requestChangesPreviewOpen)}
                    >
                      <ChevronRight
                        size={13}
                        strokeWidth={2}
                        class="review-context-chevron {requestChangesPreviewOpen ? 'review-context-chevron--open' : ''}"
                      />
                      <span>Feedback for the next attempt</span>
                      <small>
                        {(review.unresolved_comment_count ?? 0) === 1
                          ? "1 open comment"
                          : `${review.unresolved_comment_count ?? 0} open comments`}
                      </small>
                    </button>
                    {#if requestChangesPreviewOpen}
                      <pre class="review-revision-brief-body">{review.revision_brief
                        || (review.comments ?? [])
                            .filter((comment) => !comment.resolved_at)
                            .map((comment) => `${comment.path}:${comment.start_line}\n${comment.body}`)
                            .join("\n\n")
                        || "No open comments yet."}</pre>
                    {/if}
                  </section>
                {/if}

                <section class="review-context" id="review-about">
                  <button
                    type="button"
                    class="review-context-disclosure"
                    aria-expanded={reviewDetailsOpen}
                    onclick={() => {
                      reviewDetailsOpen = !reviewDetailsOpen;
                    }}
                  >
                    <ChevronRight
                      size={13}
                      strokeWidth={2}
                      class="review-context-chevron {reviewDetailsOpen ? 'review-context-chevron--open' : ''}"
                    />
                    <span>About</span>
                    <small>Base, digest, and command log</small>
                  </button>

                  {#if reviewDetailsOpen}
                    <div class="review-context-body">
                      {#if reviewDetailsLoading}
                        <div class="review-context-loading">
                          <span class="review-context-loading-dot"></span>
                          Preparing context…
                        </div>
                      {/if}

                      <ReviewProvenanceStrip
                        {review}
                        baseRef={baseRef || gitTargetBaseRef(detail?.target) || null}
                        onExport={() => void beginExport()}
                      />

                      {#if providerLinkOpen && providerHandoff?.available}
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

                      {#if commands && commands.lines.length}
                        <div class="review-context-row review-context-row--stack">
                          <button
                            type="button"
                            class="review-context-row-button"
                            aria-expanded={reviewAuditOpen}
                            onclick={() => (reviewAuditOpen = !reviewAuditOpen)}
                          >
                            <span class="review-context-icon"><GitCommitHorizontal size={14} strokeWidth={1.7} /></span>
                            <span class="review-context-copy">
                              <span class="review-context-copy-title">Command log</span>
                              <span>{commands.lines.length} recorded {commands.lines.length === 1 ? "command" : "commands"}</span>
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
                              <pre>{commands.lines.join("\n")}</pre>
                              {#if commands.truncated}
                                <button type="button" disabled={busy} onclick={() => void loadMoreCommands()}>Show more commands · {commands.lines.length} of {commands.total_lines}</button>
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

                {#if showCommentRail}
                  <ReviewCommentRail
                    comments={review.comments ?? []}
                    compose={commentCompose}
                    draft={commentDraft}
                    {busy}
                    onDraftChange={(value) => (commentDraft = value)}
                    onSubmit={submitComment}
                    onCancelCompose={() => {
                      commentCompose = null;
                      commentDraft = "";
                    }}
                    onResolve={resolveComment}
                    onDelete={removeComment}
                    onJump={(comment) => {
                      scrollToReviewFile(comment.path);
                      const el = document.querySelector(
                        `[data-diff-line="${comment.start_line}"]`,
                      ) as HTMLElement | null;
                      el?.scrollIntoView({ behavior: "smooth", block: "center" });
                    }}
                  />
                {/if}
              </div>
            </div>

            {#if reviewCanvas && review && actions?.review.allowed && (reviewNoteOpen || reviewFooterIssues.length > 0)}
              <footer class="review-decision">
                {#if reviewNoteOpen}
                  <label class="sr-only" for="review-rationale">Review note</label>
                  <textarea
                    id="review-rationale"
                    rows="2"
                    class="review-note"
                    placeholder="Anything the next person should know?"
                    bind:value={reviewRationale}
                  ></textarea>
                {/if}
                {#if reviewFooterIssues.length}
                  <div class="review-decision-row">
                    <div class="review-decision-guidance">
                      <CircleAlert size={13} strokeWidth={1.7} />
                      <p title={reviewFooterIssues.join(" · ")}>
                        {reviewIssueLabel(reviewFooterIssues[0]!)}{reviewFooterIssues.length > 1 ? ` · ${reviewFooterIssues.length - 1} more` : ""}
                      </p>
                    </div>
                  </div>
                {/if}
              </footer>
            {/if}
          </div>
        {:else if reviewCanvas}
          <div class="flex min-h-0 flex-1 items-center justify-center p-8 text-center">
            <div class="max-w-sm">
              <p class="text-sm font-medium text-surface-200">Nothing to review yet</p>
              <p class="mt-1 text-xs leading-relaxed text-content-quiet">
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
            class="absolute inset-y-0 right-0 z-30 flex w-[min(32rem,100%)] flex-col overflow-auto border-l border-surface-500/40 bg-surface-950/98 p-3 shadow-2xl"
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h4 class="text-sm font-semibold">Understand this code</h4>
                <p class="workshop-faint mt-0.5 text-[10px]">
                  See relationships and possible impact without leaving your work
                </p>
              </div>
              <div class="flex items-center gap-1 text-[10px]">
                <button
                  type="button"
                  class="rounded px-2 py-0.5 {worldSnapshot === 'baseline'
                    ? 'bg-surface-700 text-surface-50'
                    : 'text-content-tertiary'}"
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
                    : 'text-content-tertiary'}"
                  onclick={() => {
                    worldSnapshot = "sealed";
                    void loadWorldOverview();
                  }}
                >
                  Current
                </button>
                <button
                  type="button"
                  class="ml-1 rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200"
                  aria-label="Close code understanding"
                  title="Close"
                  onclick={() => (worldMode = false)}
                >×</button>
              </div>
            </div>
            <p class="mt-1 text-[10px] text-content-quiet">
              This view only explains the code; it never changes files.
            </p>

            {#if worldMapReady}
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
                  onclick={() => void rebuildWorldMap()}
                >
                  Rebuild code map
                </button>
              </div>
              {#if worldBinding}
                <details class="mt-2 text-[10px] text-content-quiet">
                  <summary class="w-fit cursor-pointer hover:text-content-secondary">Technical details</summary>
                  <p class="mt-1">
                    Before: {worldBinding.baseline?.state ?? "not indexed"} · current:
                    {worldBinding.sealed?.state ?? "not indexed"}
                  </p>
                  {#if worldBinding.capabilities}
                    <div class="mt-1 flex flex-wrap gap-1">
                    {#each Object.entries(worldBinding.capabilities).filter(([key]) => key !== "note") as [capability, enabled]}
                      <span
                        class="rounded-full border border-surface-500/30 px-1.5 py-0.5 text-[9px] {enabled
                          ? 'text-content-secondary'
                          : 'text-content-faint'}"
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
                    <p class="p-2 text-[10px] text-content-quiet">Nothing matched that name.</p>
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
                          <span class="block truncate font-mono text-[9px] text-content-quiet">{entity.path}</span>
                        </span>
                        <span class="shrink-0 text-[9px] text-content-quiet">{entity.kind}</span>
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
                    <p class="mt-1 text-[10px] text-content-quiet">{worldImpact.message}</p>
                  {/if}
                  <ul class="mt-1 max-h-32 overflow-auto text-[10px] text-content-tertiary">
                    {#each worldImpact.nodes as node (node.id)}
                      <li class="truncate py-0.5">{node.label} <span class="text-content-faint">· {node.path}</span></li>
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
                    <p class="text-[9px] text-content-quiet">fully understood</p>
                  </div>
                  <div class="rounded-md bg-surface-900/60 p-2">
                    <p class="text-lg font-semibold text-surface-100">
                      {worldInsight.code_avec?.scoreable_entities ?? 0}
                    </p>
                    <p class="text-[9px] text-content-quiet">code elements found</p>
                  </div>
                  <div class="rounded-md bg-surface-900/60 p-2">
                    <p class="text-lg font-semibold text-surface-100">
                      {worldInsight.code_avec?.gaps.length ?? 0}
                    </p>
                    <p class="text-[9px] text-content-quiet">still unclear</p>
                  </div>
                </div>
              {/if}
              {#if worldFiles}
                <details class="mt-2">
                  <summary class="cursor-pointer text-[10px] text-content-tertiary">
                    Files in this view · {worldFiles.files.length}
                  </summary>
                  <ul class="mt-1 max-h-48 overflow-auto rounded-md border border-surface-500/25">
                    {#each worldFiles.files as file (file.id)}
                      <li class="border-b border-surface-500/15 px-2 py-1 last:border-0">
                        <button
                          type="button"
                          class="w-full truncate text-left font-mono text-[10px] text-content-tertiary hover:text-surface-100"
                          onclick={() =>
                            void revealLocation({ path: file.path, line: 1, entityId: file.id })}
                        >{file.path}</button>
                      </li>
                    {/each}
                  </ul>
                </details>
              {/if}
            {:else}
              <div class="mt-6 flex flex-1 flex-col items-center justify-center px-2">
                {#if busy || worldMapIndexing}
                  <p class="workshop-faint text-sm">Building the code map…</p>
                  <p class="mt-1 max-w-xs text-center text-[10px] leading-relaxed text-content-quiet">
                    Relationships and impact stay hidden until indexing finishes.
                  </p>
                {:else}
                  <EmptyState
                    title={worldMapFailed ? "Code map failed" : "Code map isn’t ready"}
                    description={worldError
                      ? humanizeForgeMessage(worldError)
                      : worldMapFailed
                        ? "Indexing didn’t finish. Rebuild the code map and try again."
                        : "Build a map of this project to find symbols and see what depends on them."}
                  >
                    <div class="flex flex-wrap items-center justify-center gap-2">
                      <button
                        type="button"
                        class="rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50"
                        disabled={busy}
                        onclick={() => void rebuildWorldMap()}
                      >Rebuild code map</button>
                      <button
                        type="button"
                        class="rounded border border-surface-500/40 px-3 py-1.5 text-[11px] text-surface-200 hover:bg-surface-800"
                        disabled={busy}
                        onclick={() => void loadWorldOverview()}
                      >Refresh</button>
                    </div>
                  </EmptyState>
                {/if}
              </div>
              {#if worldBinding}
                <details class="mt-auto pt-4 text-[10px] text-content-quiet">
                  <summary class="w-fit cursor-pointer hover:text-content-secondary">Technical details</summary>
                  <p class="mt-1">
                    Before: {worldBinding.baseline?.state ?? "not indexed"} · current:
                    {worldBinding.sealed?.state ?? "not indexed"}
                  </p>
                </details>
              {/if}
            {/if}
          </div>
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

  .review-canvas {
    width: 100%;
    max-width: 88rem;
    margin: 0 auto;
  }

  .review-canvas--with-rail {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(14rem, 17rem);
    gap: 0.85rem;
    align-items: start;
  }

  .review-canvas-main {
    min-width: 0;
  }

  .review-revision-brief {
    margin-top: 0.85rem;
  }

  .review-revision-brief-body {
    margin-top: 0.45rem;
    max-height: 12rem;
    overflow: auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-950) / 0.35);
    padding: 0.65rem 0.75rem;
    color: rgb(var(--theme-text-secondary));
    font-family: var(--font-mono);
    font-size: 0.625rem;
    line-height: 1.45;
    white-space: pre-wrap;
  }

  @media (max-width: 960px) {
    .review-canvas--with-rail {
      grid-template-columns: 1fr;
    }
  }

  .review-policy {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    margin-top: 0.85rem;
    border: 1px solid rgb(var(--color-warning-500) / 0.26);
    border-radius: 0.6rem;
    padding: 0.75rem;
    background: rgb(var(--color-warning-500) / 0.1);
    color: rgb(var(--theme-warning));
  }

  .review-policy p {
    font-size: 0.6875rem;
    font-weight: 600;
  }

  .review-policy ul {
    margin-top: 0.3rem;
    font-size: 0.625rem;
    line-height: 1.5;
    color: rgb(var(--theme-warning) / 0.72);
  }

  .review-policy-path {
    font-family: var(--font-mono);
  }

  .review-policy-path {
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    text-decoration: underline;
    text-underline-offset: 0.12em;
    cursor: pointer;
  }

  .review-policy-path:hover {
    color: rgb(var(--color-surface-100));
  }

  .review-policy label {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    margin-top: 0.55rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-warning));
  }

  .review-prior-note {
    margin-top: 0.75rem;
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-prior-note span {
    color: rgb(var(--color-surface-200));
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
    color: rgb(var(--theme-text-secondary));
    text-align: left;
    transition: color 140ms ease, background-color 140ms ease;
  }

  .review-context-disclosure:hover {
    background: rgb(var(--color-surface-800) / 0.3);
    color: rgb(var(--theme-text));
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
    color: rgb(var(--theme-text-quiet));
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
    color: rgb(var(--theme-text-quiet));
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
    color: rgb(var(--theme-text-quiet));
  }

  .review-context-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
  }

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
    font-size: 0.6875rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .review-audit button {
    border: 0;
    background: transparent;
    padding: 0.15rem 0.3rem;
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-audit button:hover {
    color: rgb(var(--theme-link));
  }

  .review-link-compose {
    display: flex;
    max-width: 34rem;
    align-items: center;
    gap: 0.4rem;
    margin: 0.45rem 0 0;
    border-radius: 0.45rem;
    padding: 0.3rem 0.4rem;
    background: rgb(var(--color-surface-800) / 0.28);
    color: rgb(var(--theme-text-quiet));
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
    color: rgb(var(--theme-text-faint));
  }

  .review-link-compose button {
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.25rem 0.4rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-link));
  }

  .review-link-compose button:disabled {
    color: rgb(var(--theme-text-faint));
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

  .review-audit {
    margin: 0.55rem 0 0 2.3rem;
  }

  .review-audit-revision {
    margin-bottom: 0.35rem;
    font-family: var(--font-mono);
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
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
    color: rgb(var(--theme-text-tertiary));
  }

  .review-exported {
    margin: 0.4rem 0;
    font-size: 0.625rem;
    color: rgb(var(--theme-success));
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
    color: rgb(var(--theme-text-faint));
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
    color: rgb(var(--theme-warning));
  }

  .review-decision-guidance p {
    overflow: hidden;
    font-size: 0.8125rem;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    color: rgb(var(--theme-link));
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
    color: rgb(var(--theme-text-quiet));
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
    color: rgb(var(--theme-text-tertiary));
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
    color: rgb(var(--theme-text-faint));
  }

  .review-export-dialog small {
    margin-top: -0.45rem;
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
  }

  .review-export-dialog .review-export-path {
    overflow: hidden;
    margin-top: 0.35rem;
    border-radius: 0.45rem;
    padding: 0.5rem 0.6rem;
    background: rgb(var(--color-surface-800) / 0.35);
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: rgb(var(--theme-text-tertiary));
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
    color: rgb(var(--theme-text-tertiary));
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
    .review-context-row-button {
      grid-template-columns: 1.75rem minmax(0, 1fr);
    }

    :global(.review-context-row-chevron) {
      grid-column: 2;
      justify-self: flex-start;
    }

    .review-decision-row {
      align-items: flex-start;
      flex-direction: column;
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
