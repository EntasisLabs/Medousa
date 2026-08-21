/**
 * Project task / test / output pane mode.
 * CodeSourceEditor wires layout; this owns run state, buffers, and actions.
 */

import {
  cancelProjectTaskRun,
  getProjectTaskRun,
  getProjectTaskRuns,
  getProjectTasks,
  getProjectTests,
  startProjectTaskRun,
  isMissingForgeRoute,
  type ProjectTask,
  type ProjectTaskResult,
  type ProjectTaskRun,
  type ProjectTaskRunSummary,
  type ProjectTest,
} from "$lib/code/codeDocumentService";
import {
  CodeTaskRunEventStream,
  type ProjectTaskOutputEvent,
} from "$lib/code/codeTaskRunEvents";
import { resolveTaskPreviewOpenUrl } from "$lib/code/taskPreviewUrl";
import { openInBrowser } from "$lib/utils/openInBrowser";
import { deferCodeWorkspaceWork } from "$lib/utils/codeWorkspaceTrace";

export type CodeTaskLocation = {
  path: string;
  line: number;
  column?: number | null;
  message: string;
};

export type CodeTasksLease = { leaseId: string; generation: number };

export type CodeTasksControllerDeps = {
  getWorkId: () => string;
  persistTestsOpen: (open: boolean) => void;
  persistOutputOpen: (open: boolean) => void;
  persistSelectedTask: (taskId: string | null) => void;
  persistRunRefs: (activeRunId: string | null, recentRunIds: string[]) => void;
  prepareRun: () => Promise<boolean>;
  ensureLease: () => Promise<CodeTasksLease>;
  onError: (message: string) => void;
  onOpenTerminal?: () => void;
  refreshDetail: () => Promise<void>;
};

export class CodeTasksController {
  projectTasks = $state<ProjectTask[]>([]);
  selectedTaskId = $state("");
  preparing = $state(false);
  running = $state(false);
  result = $state<ProjectTaskResult | null>(null);
  run = $state<ProjectTaskRun | null>(null);
  outputOpen = $state(false);
  liveStdout = $state("");
  liveStderr = $state("");
  outputTruncated = $state(false);
  liveLocations = $state<CodeTaskLocation[]>([]);
  readyUrl = $state<string | null>(null);
  previewOpening = $state(false);
  projectTests = $state<ProjectTest[]>([]);
  testsOpen = $state(false);
  catalogError = $state<string | null>(null);
  recentRuns = $state<ProjectTaskRunSummary[]>([]);
  runHistoryTruncated = $state(false);
  runRegistryEvictedCount = $state(0);
  runListingSupported = $state<boolean | null>(null);

  /** Exact prior invocation, including a targeted test, for truthful reruns. */
  lastInvocation = $state<{ taskId: string; testId?: string } | null>(null);
  restoredTaskId = $state<string | null>(null);
  restoredActiveRunId = $state<string | null>(null);
  restoredRecentRunIds = $state<string[]>([]);

  #eventStream: CodeTaskRunEventStream | null = null;
  #deps: CodeTasksControllerDeps;
  #boundWorkId = "";
  #monitorGeneration = 0;

  constructor(deps: CodeTasksControllerDeps) {
    this.#deps = deps;
  }

  get selectedTask(): ProjectTask | null {
    return (
      this.projectTasks.find((task) => task.id === this.selectedTaskId) ??
      this.projectTasks[0] ??
      null
    );
  }

  taskAvailable(task: ProjectTask | null | undefined): boolean {
    return task?.available !== false;
  }

  taskRepair(task: ProjectTask | null | undefined): string | null {
    return task?.requirements?.find((requirement) => !requirement.available)?.repair ?? null;
  }

  defaultTask(tasks = this.projectTasks): ProjectTask | null {
    return (
      [...tasks]
        .filter((task) => this.taskAvailable(task))
        .sort((left, right) => (right.default_rank ?? 0) - (left.default_rank ?? 0))[0] ??
      tasks[0] ??
      null
    );
  }

  restoreTestsOpen(open: boolean) {
    this.testsOpen = open;
  }

  restoreOutputOpen(open: boolean) {
    this.outputOpen = open;
  }

  restoreSelectedTask(taskId: string | null | undefined) {
    this.restoredTaskId = taskId?.trim() || null;
    if (!this.restoredTaskId) return;
    if (this.projectTasks.some((task) => task.id === this.restoredTaskId)) {
      this.selectedTaskId = this.restoredTaskId;
    }
  }

  restoreRunRefs(activeRunId: string | null | undefined, recentRunIds: string[] | undefined) {
    this.restoredActiveRunId = activeRunId?.trim() || null;
    this.restoredRecentRunIds = (recentRunIds ?? [])
      .map((runId) => runId.trim())
      .filter(Boolean)
      .slice(0, 12);
  }

  selectTask(taskId: string) {
    if (!this.projectTasks.some((task) => task.id === taskId)) return;
    this.selectedTaskId = taskId;
    this.restoredTaskId = taskId;
    this.#deps.persistSelectedTask(taskId);
  }

  toggleOutput(forceOpen?: boolean) {
    this.outputOpen =
      forceOpen === true ? true : forceOpen === false ? false : !this.outputOpen;
    this.#deps.persistOutputOpen(this.outputOpen);
  }

  persistRunRefs() {
    const activeRunId = this.run && this.runStillActive(this.run) ? this.run.run_id : null;
    const recentRunIds = [
      ...(this.run ? [this.run.run_id] : []),
      ...this.recentRuns.map((run) => run.run_id),
    ].filter((runId, index, all) => all.indexOf(runId) === index).slice(0, 12);
    this.#deps.persistRunRefs(activeRunId, recentRunIds);
  }

  resetOutputBuffers(run?: ProjectTaskRun | null) {
    this.liveStdout = run?.stdout ?? "";
    this.liveStderr = run?.stderr ?? "";
    this.outputTruncated = run?.output_truncated ?? false;
    this.liveLocations = run?.locations ?? [];
    this.readyUrl = run?.ready_url ?? null;
  }

  mergeLocations(incoming: CodeTaskLocation[] | null | undefined) {
    if (!incoming?.length) return;
    const next = [...this.liveLocations];
    for (const location of incoming) {
      if (
        next.some(
          (existing) =>
            existing.path === location.path &&
            existing.line === location.line &&
            (existing.column ?? null) === (location.column ?? null),
        )
      ) {
        continue;
      }
      next.push(location);
      if (next.length >= 100) break;
    }
    this.liveLocations = next;
  }

  applyRunEvent(event: ProjectTaskOutputEvent) {
    if (event.kind === "output" && event.text) {
      if (event.stream === "stderr") {
        this.liveStderr += event.text;
      } else {
        this.liveStdout += event.text;
      }
      this.mergeLocations(event.locations);
      if (this.run && event.run_id === this.run.run_id) {
        this.run = {
          ...this.run,
          stdout: this.liveStdout,
          stderr: this.liveStderr,
          locations: this.liveLocations,
          next_seq: event.seq + 1,
        };
      }
      return;
    }
    if (event.kind === "state") {
      this.mergeLocations(event.locations);
      if (event.ready_url) this.readyUrl = event.ready_url;
      if (this.run && event.run_id === this.run.run_id) {
        this.run = {
          ...this.run,
          state: event.state ?? this.run.state,
          result: event.result ?? this.run.result,
          stdout: event.result?.stdout ?? this.liveStdout,
          stderr: event.result?.stderr ?? this.liveStderr,
          output_truncated: event.result?.truncated ?? this.outputTruncated,
          locations: event.result?.locations ?? this.liveLocations,
          ready_url: event.ready_url ?? this.run.ready_url ?? this.readyUrl,
          next_seq: event.seq + 1,
        };
      }
      if (event.result) {
        this.liveStdout = event.result.stdout;
        this.liveStderr = event.result.stderr;
        this.outputTruncated = event.result.truncated;
        this.liveLocations = event.result.locations ?? this.liveLocations;
        this.result = event.result;
        this.persistRunRefs();
      } else if (event.state) {
        if (this.run) this.run = { ...this.run, state: event.state };
      }
    }
  }

  stopRunEvents() {
    this.#eventStream?.teardown();
    this.#eventStream = null;
  }

  startRunEvents(workId: string, run: ProjectTaskRun, since = 0) {
    this.stopRunEvents();
    this.resetOutputBuffers(run);
    const stream = new CodeTaskRunEventStream({
      onEvent: (event) => this.applyRunEvent(event),
      onUnavailable: () => {
        /* polling fallback in runDetected */
      },
      onTerminal: (result, state) => {
        if (result) {
          this.result = result;
          this.liveStdout = result.stdout;
          this.liveStderr = result.stderr;
          this.outputTruncated = result.truncated;
        }
        if (this.run) {
          this.run = {
            ...this.run,
            state: state ?? this.run.state,
            result: result ?? this.run.result,
            stdout: result?.stdout ?? this.liveStdout,
            stderr: result?.stderr ?? this.liveStderr,
            output_truncated: result?.truncated ?? this.outputTruncated,
          };
        }
      },
    });
    this.#eventStream = stream;
    stream.start(workId, run.run_id, since);
  }

  applyRunSnapshot(snapshot: ProjectTaskRun) {
    this.run = {
      ...snapshot,
      stdout: snapshot.stdout || this.liveStdout || snapshot.stdout,
      stderr: snapshot.stderr || this.liveStderr || snapshot.stderr,
    };
    if (snapshot.stdout) this.liveStdout = snapshot.stdout;
    if (snapshot.stderr) this.liveStderr = snapshot.stderr;
    this.outputTruncated = snapshot.output_truncated ?? this.outputTruncated;
    if (snapshot.locations?.length) this.mergeLocations(snapshot.locations);
    if (snapshot.ready_url) this.readyUrl = snapshot.ready_url;
    if (snapshot.result) {
      this.result = snapshot.result;
      this.liveStdout = snapshot.result.stdout;
      this.liveStderr = snapshot.result.stderr;
      this.outputTruncated = snapshot.result.truncated;
      this.liveLocations = snapshot.result.locations ?? this.liveLocations;
    }
    const summary: ProjectTaskRunSummary = {
      run_id: snapshot.run_id,
      work_id: snapshot.work_id,
      state: snapshot.state,
      task: snapshot.task,
      test_id: snapshot.test_id,
      started_at: snapshot.started_at ?? new Date().toISOString(),
      finished_at: snapshot.finished_at,
      terminal: !this.runStillActive(snapshot),
      output_truncated: snapshot.output_truncated ?? false,
      next_seq: snapshot.next_seq ?? 0,
      ready_url: snapshot.ready_url,
    };
    this.recentRuns = [summary, ...this.recentRuns.filter((run) => run.run_id !== summary.run_id)]
      .sort((left, right) => right.started_at.localeCompare(left.started_at))
      .slice(0, 20);
  }

  async monitorActiveRun(workId: string, runId: string) {
    const generation = ++this.#monitorGeneration;
    try {
      while (
        generation === this.#monitorGeneration &&
        this.#boundWorkId === workId &&
        this.run?.run_id === runId &&
        this.runStillActive(this.run)
      ) {
        await new Promise((resolve) => setTimeout(resolve, 350));
        const snapshot = await getProjectTaskRun(workId, runId);
        if (generation !== this.#monitorGeneration || this.#boundWorkId !== workId) return;
        this.applyRunSnapshot(snapshot);
      }
      if (this.run?.run_id === runId) this.result = this.run.result ?? this.result;
      this.persistRunRefs();
      await this.#deps.refreshDetail();
    } catch (err) {
      if (generation === this.#monitorGeneration) {
        this.#deps.onError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      if (generation === this.#monitorGeneration) {
        this.stopRunEvents();
        this.running = false;
      }
    }
  }

  async hydrateTaskRuns(workId: string) {
    try {
      const listing = await getProjectTaskRuns(workId);
      const summaries = listing.runs;
      if (this.#boundWorkId !== workId) return;
      this.runListingSupported = true;
      this.recentRuns = summaries;
      this.runHistoryTruncated = listing.truncated;
      this.runRegistryEvictedCount = listing.registry_evicted_count;
      this.persistRunRefs();
      const selected = summaries.find((run) => !run.terminal) ?? summaries[0];
      if (!selected || this.run?.run_id === selected.run_id) return;
      const snapshot = await getProjectTaskRun(workId, selected.run_id);
      if (this.#boundWorkId !== workId) return;
      this.applyRunSnapshot(snapshot);
      this.persistRunRefs();
      this.lastInvocation = {
        taskId: snapshot.task.id,
        ...(snapshot.test_id ? { testId: snapshot.test_id } : {}),
      };
      if (this.runStillActive(snapshot)) {
        this.running = true;
        this.toggleOutput(true);
        this.startRunEvents(workId, snapshot, snapshot.next_seq ?? 0);
        void this.monitorActiveRun(workId, snapshot.run_id);
      }
    } catch (err) {
      if (this.#boundWorkId !== workId) return;
      if (isMissingForgeRoute(err)) {
        this.runListingSupported = false;
        this.recentRuns = [];
        this.runHistoryTruncated = false;
        this.runRegistryEvictedCount = 0;
        const fallbackRunId = this.restoredActiveRunId ?? this.restoredRecentRunIds[0];
        if (fallbackRunId) {
          try {
            const snapshot = await getProjectTaskRun(workId, fallbackRunId);
            if (this.#boundWorkId !== workId) return;
            this.applyRunSnapshot(snapshot);
            this.lastInvocation = {
              taskId: snapshot.task.id,
              ...(snapshot.test_id ? { testId: snapshot.test_id } : {}),
            };
            if (this.runStillActive(snapshot)) {
              this.running = true;
              this.toggleOutput(true);
              this.startRunEvents(workId, snapshot, snapshot.next_seq ?? 0);
              void this.monitorActiveRun(workId, snapshot.run_id);
            }
          } catch {
            // The legacy daemon may have evicted the saved run; task execution still works.
          }
        }
        return;
      }
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    }
  }

  runStillActive(run: ProjectTaskRun): boolean {
    if (run.state === "running" || run.state === "ready") return true;
    // Cancel flips state before the process exits and final result lands.
    if (run.state === "cancelled" && !run.result) return true;
    return false;
  }

  async runDetected(test?: ProjectTest) {
    const taskId = test?.task_id ?? this.selectedTask?.id;
    if (!taskId) {
      this.#deps.onOpenTerminal?.();
      return;
    }
    if (this.running || this.preparing) return;
    await this.runInvocation({ taskId, testId: test?.id });
  }

  async runKind(kind: "run" | "build" | "test" | "verify") {
    const candidates = this.projectTasks.filter((candidate) => candidate.kind === kind);
    const task = this.defaultTask(candidates);
    if (!task) {
      this.#deps.onError(`No ${kind} command was detected for this project.`);
      return;
    }
    if (!this.taskAvailable(task)) {
      this.#deps.onError(this.taskRepair(task) ?? `${task.label} is unavailable.`);
      return;
    }
    if (this.running || this.preparing) return;
    this.selectTask(task.id);
    await this.runInvocation({ taskId: task.id });
  }

  async rerunLast() {
    if (!this.lastInvocation || this.running || this.preparing) return;
    await this.runInvocation(this.lastInvocation);
  }

  clearOutput() {
    this.liveStdout = "";
    this.liveStderr = "";
    this.liveLocations = [];
    this.outputTruncated = false;
  }

  async runInvocation(invocation: { taskId: string; testId?: string }) {
    if (this.running || this.preparing) return;
    const task = this.projectTasks.find((candidate) => candidate.id === invocation.taskId);
    if (task && !this.taskAvailable(task)) {
      this.#deps.onError(this.taskRepair(task) ?? `${task.label} is unavailable.`);
      return;
    }
    const workId = this.#deps.getWorkId();
    this.#deps.onError("");
    this.preparing = true;
    try {
      if (!(await this.#deps.prepareRun())) {
        this.preparing = false;
        return;
      }
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
      this.preparing = false;
      return;
    }
    this.preparing = false;
    this.running = true;
    this.result = null;
    // Long-running apps need a visible control surface. Short verification
    // stays quiet unless the user already opened Output; failures promote
    // their matcher diagnostics into Problems at the workbench layer.
    if (task?.long_running || task?.background || task?.kind === "run") {
      this.toggleOutput(true);
    }
    try {
      const lease = await this.#deps.ensureLease();
      this.run = await startProjectTaskRun(
        workId,
        invocation.taskId,
        {
          lease_id: lease.leaseId,
          generation: lease.generation,
          test_id: invocation.testId,
        },
      );
      this.persistRunRefs();
      this.lastInvocation = { ...invocation };
      this.startRunEvents(workId, this.run);
      await this.monitorActiveRun(workId, this.run.run_id);
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      if (!this.run || !this.runStillActive(this.run)) {
        this.stopRunEvents();
        this.running = false;
      }
    }
  }

  async openPreview() {
    if (!(this.readyUrl || this.run?.ready_url) || this.previewOpening) return;
    this.previewOpening = true;
    this.#deps.onError("");
    try {
      if (!this.run) throw new Error("No task run is available");
      const workId = this.#deps.getWorkId();
      const { url } = await resolveTaskPreviewOpenUrl(workId, {
        ...this.run,
        ready_url: this.readyUrl ?? this.run.ready_url,
      });
      await openInBrowser(url, {
        openedBy: "user",
        workCardId: workId,
        title: this.run.task.label,
      });
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    } finally {
      this.previewOpening = false;
    }
  }

  async openRun(runId: string) {
    if (!runId || this.running || this.run?.run_id === runId) return;
    try {
      const snapshot = await getProjectTaskRun(this.#deps.getWorkId(), runId);
      this.resetOutputBuffers(snapshot);
      this.applyRunSnapshot(snapshot);
      this.lastInvocation = {
        taskId: snapshot.task.id,
        ...(snapshot.test_id ? { testId: snapshot.test_id } : {}),
      };
      this.toggleOutput(true);
      this.persistRunRefs();
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    }
  }

  async stopDetected() {
    if (!this.run || (this.run.state !== "running" && this.run.state !== "ready")) {
      return;
    }
    try {
      this.run = await cancelProjectTaskRun(this.#deps.getWorkId(), this.run.run_id);
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    }
  }

  async toggleTests() {
    const next = !this.testsOpen;
    this.testsOpen = next;
    this.#deps.persistTestsOpen(next);
    const workId = this.#deps.getWorkId();
    if (!next || this.projectTests.length || !workId) return;
    try {
      this.projectTests = await getProjectTests(workId);
    } catch (err) {
      this.#deps.onError(err instanceof Error ? err.message : String(err));
    }
  }

  bindTaskList(workId: string, prepared: boolean, interactive: boolean): () => void {
    if (workId !== this.#boundWorkId) {
      this.#monitorGeneration += 1;
      this.stopRunEvents();
      this.#boundWorkId = workId;
      this.restoredTaskId = null;
      this.restoredActiveRunId = null;
      this.restoredRecentRunIds = [];
      this.lastInvocation = null;
      this.result = null;
      this.run = null;
      this.recentRuns = [];
      this.runHistoryTruncated = false;
      this.runRegistryEvictedCount = 0;
      this.runListingSupported = null;
      this.resetOutputBuffers();
    }
    if (!interactive || !workId || !prepared) {
      this.projectTasks = [];
      this.selectedTaskId = "";
      this.catalogError = null;
      return () => {};
    }
    let cancelled = false;
    const cancelDeferred = deferCodeWorkspaceWork(() => {
      void this.hydrateTaskRuns(workId);
      void getProjectTasks(workId)
        .then((loaded) => {
          if (cancelled) return;
          this.projectTasks = loaded;
          this.catalogError = null;
          if (
            this.restoredTaskId &&
            loaded.some((task) => task.id === this.restoredTaskId)
          ) {
            this.selectedTaskId = this.restoredTaskId;
            return;
          }
          if (!loaded.some((task) => task.id === this.selectedTaskId)) {
            this.selectedTaskId = this.defaultTask(loaded)?.id ?? "";
          }
        })
        .catch((err) => {
          if (cancelled) return;
          this.projectTasks = [];
          this.selectedTaskId = "";
          this.catalogError = err instanceof Error ? err.message : String(err);
          this.#deps.onError(this.catalogError);
        });
    });
    return () => {
      cancelled = true;
      cancelDeferred();
    };
  }

  dispose() {
    this.#monitorGeneration += 1;
    this.stopRunEvents();
  }
}
