import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  startProjectTaskRun: vi.fn(),
  getProjectTaskRun: vi.fn(),
  cancelProjectTaskRun: vi.fn(),
  getProjectTasks: vi.fn(),
  getProjectTests: vi.fn(),
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
    persistSelectedTask,
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
});
