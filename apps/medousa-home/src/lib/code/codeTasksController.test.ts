import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  startProjectTaskRun: vi.fn(),
  getProjectTaskRun: vi.fn(),
  getProjectTaskRuns: vi.fn(),
  cancelProjectTaskRun: vi.fn(),
  getProjectTasks: vi.fn(),
  getProjectTests: vi.fn(),
  isMissingForgeRoute: vi.fn((err: unknown) => (err as { status?: number })?.status === 404),
}));

vi.mock("$lib/code/codeDocumentService", () => api);
vi.mock("$lib/code/codeTaskRunEvents", () => ({
  CodeTaskRunEventStream: class {
    start() {}
    teardown() {}
  },
}));
vi.mock("$lib/code/taskPreviewUrl", () => ({
  resolveTaskPreviewOpenUrl: vi.fn(),
}));
vi.mock("$lib/utils/openInBrowser", () => ({ openInBrowser: vi.fn() }));
vi.mock("$lib/utils/codeWorkspaceTrace", () => ({
  deferCodeWorkspaceWork: (task: () => void) => {
    task();
    return () => {};
  },
}));

import { CodeTasksController } from "./codeTasksController.svelte";

const checkTask = {
  id: "cargo-check",
  label: "Check",
  kind: "verify",
  argv: ["cargo", "check"],
  provider: "cargo",
};

function createController(overrides?: { prepareRun?: () => Promise<boolean> }) {
  const persistSelectedTask = vi.fn();
  const ensureLease = vi.fn(async () => ({ leaseId: "lease-1", generation: 3 }));
  const refreshDetail = vi.fn(async () => {});
  const onError = vi.fn();
  const controller = new CodeTasksController({
    getWorkId: () => "work-1",
    persistTestsOpen: vi.fn(),
    persistOutputOpen: vi.fn(),
    persistSelectedTask,
    persistRunRefs: vi.fn(),
    prepareRun: overrides?.prepareRun ?? vi.fn(async () => true),
    ensureLease,
    onError,
    refreshDetail,
  });
  controller.projectTasks = [checkTask];
  controller.selectedTaskId = checkTask.id;
  return { controller, persistSelectedTask, ensureLease, refreshDetail, onError };
}

describe("CodeTasksController", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.startProjectTaskRun.mockResolvedValue({
      run_id: "run-1",
      work_id: "work-1",
      state: "passed",
      task: checkTask,
      result: null,
      stdout: "",
      stderr: "",
      locations: [],
    });
    api.getProjectTaskRuns.mockResolvedValue({
      runs: [],
      truncated: false,
      retained_count: 0,
      active_count: 0,
      terminal_count: 0,
      terminal_limit: 64,
      terminal_ttl_seconds: 600,
      registry_evicted_count: 0,
    });
    api.getProjectTasks.mockResolvedValue([checkTask]);
  });

  it("does not start a task when saving dirty buffers is blocked", async () => {
    const prepareRun = vi.fn(async () => false);
    const { controller, ensureLease } = createController({ prepareRun });

    await controller.runDetected();

    expect(prepareRun).toHaveBeenCalledOnce();
    expect(ensureLease).not.toHaveBeenCalled();
    expect(api.startProjectTaskRun).not.toHaveBeenCalled();
    expect(controller.running).toBe(false);
  });

  it("reruns the exact targeted test invocation", async () => {
    const { controller } = createController();
    const test = {
      id: "src/lib.rs::saves_drafts",
      label: "saves_drafts",
      path: "src/lib.rs",
      line: 42,
      task_id: checkTask.id,
    };

    await controller.runDetected(test);
    await controller.rerunLast();

    expect(api.startProjectTaskRun).toHaveBeenCalledTimes(2);
    expect(api.startProjectTaskRun).toHaveBeenNthCalledWith(
      2,
      "work-1",
      checkTask.id,
      {
        lease_id: "lease-1",
        generation: 3,
        test_id: test.id,
      },
    );
  });

  it("persists an explicitly selected project task", () => {
    const { controller, persistSelectedTask } = createController();

    controller.selectTask(checkTask.id);

    expect(controller.selectedTaskId).toBe(checkTask.id);
    expect(persistSelectedTask).toHaveBeenCalledWith(checkTask.id);
  });

  it("ranks healthy provider recommendations above catalog order", () => {
    const { controller } = createController();
    const build = { ...checkTask, id: "cargo-build", kind: "build", default_rank: 300 };
    const run = { ...checkTask, id: "cargo-run", kind: "run", default_rank: 450 };
    const unavailableDev = {
      ...checkTask,
      id: "npm-dev",
      kind: "run",
      default_rank: 500,
      available: false,
    };

    expect(controller.defaultTask([build, unavailableDev, run])?.id).toBe("cargo-run");
  });

  it("surfaces executable repair before save or lease work", async () => {
    const prepareRun = vi.fn(async () => true);
    const { controller, ensureLease, onError } = createController({ prepareRun });
    controller.projectTasks = [{
      ...checkTask,
      available: false,
      requirements: [{
        kind: "executable",
        name: "cargo",
        available: false,
        repair: "Install cargo on the workshop machine.",
      }],
    }];

    await controller.runDetected();

    expect(onError).toHaveBeenCalledWith("Install cargo on the workshop machine.");
    expect(prepareRun).not.toHaveBeenCalled();
    expect(ensureLease).not.toHaveBeenCalled();
  });

  it("hydrates an active daemon run after the Code surface remounts", async () => {
    const { controller } = createController();
    const active = {
      run_id: "run-active",
      work_id: "work-1",
      state: "running",
      task: checkTask,
      test_id: "src/lib.rs::checks",
      started_at: "2026-08-21T00:00:00Z",
      terminal: false,
      output_truncated: false,
      next_seq: 4,
    };
    api.getProjectTaskRuns.mockResolvedValue({
      runs: [active],
      truncated: false,
      retained_count: 1,
      active_count: 1,
      terminal_count: 0,
      terminal_limit: 64,
      terminal_ttl_seconds: 600,
      registry_evicted_count: 0,
    });
    api.getProjectTaskRun.mockResolvedValue({
      ...active,
      stdout: "building\n",
      stderr: "",
      locations: [],
      result: null,
    });

    const unbind = controller.bindTaskList("work-1", true, true);

    await vi.waitFor(() => expect(controller.run?.run_id).toBe("run-active"));
    expect(controller.liveStdout).toBe("building\n");
    expect(controller.running).toBe(true);
    expect(controller.lastInvocation).toEqual({
      taskId: checkTask.id,
      testId: "src/lib.rs::checks",
    });
    unbind();
    controller.dispose();
  });

  it("falls back cleanly when an older daemon has no run listing route", async () => {
    const { controller, onError } = createController();
    api.getProjectTaskRuns.mockRejectedValue(Object.assign(new Error("missing"), { status: 404 }));

    const unbind = controller.bindTaskList("work-1", true, true);

    await vi.waitFor(() => expect(controller.runListingSupported).toBe(false));
    expect(onError).not.toHaveBeenCalled();
    unbind();
  });

  it("uses a persisted active run reference with a legacy daemon", async () => {
    const { controller } = createController();
    api.getProjectTaskRuns.mockRejectedValue(Object.assign(new Error("missing"), { status: 404 }));
    api.getProjectTaskRun.mockResolvedValue({
      run_id: "run-saved",
      work_id: "work-1",
      state: "passed",
      task: checkTask,
      test_id: null,
      started_at: "2026-08-21T00:00:00Z",
      finished_at: "2026-08-21T00:00:01Z",
      result: {
        task: checkTask,
        success: true,
        stdout: "ok\n",
        stderr: "",
        truncated: false,
        duration_ms: 1000,
        locations: [],
      },
      stdout: "ok\n",
      stderr: "",
      locations: [],
      next_seq: 2,
    });
    const unbind = controller.bindTaskList("work-1", true, true);
    controller.restoreRunRefs("run-saved", ["run-saved"]);
    await controller.hydrateTaskRuns("work-1");

    expect(controller.run?.run_id).toBe("run-saved");
    expect(controller.result?.success).toBe(true);
    unbind();
    controller.dispose();
  });
});
