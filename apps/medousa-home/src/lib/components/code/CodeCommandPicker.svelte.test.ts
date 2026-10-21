/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import CodeCommandPicker from "./CodeCommandPicker.svelte";
import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
import { CodeTasksController as Tasks } from "$lib/code/codeTasksController.svelte";
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });
it("bounds the catalog and selects with the keyboard before running", async () => {
  const selectTask = vi.fn();
  const tasks = { projectTasks: Array.from({ length: 3000 }, (_, i) => ({ id: `task-${i}`, label: `Run ${i}`, argv: ["tool", `${i}`], root: "app", kind: "run", provider: "runtime" })), selectedTask: null, selectedTaskId: "", selectTask, taskRepair: () => null } as unknown as CodeTasksController;
  component = mount(CodeCommandPicker, { target: document.body, props: { tasks, path: "app/main.ts" } });
  flushSync();
  document.querySelector<HTMLButtonElement>('[aria-label="Choose project command"]')!.click();
  await vi.waitFor(() => expect(document.querySelectorAll("[data-command]").length).toBe(60));
  const input = document.querySelector<HTMLInputElement>('[aria-label="Search project commands"]')!;
  input.value = "2999"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(document.querySelectorAll("[data-command]")).toHaveLength(1);
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); flushSync();
  expect(selectTask).toHaveBeenCalledWith("task-2999");
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Choose project command");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

function runController() {
  const tasks = new Tasks({
    getWorkId: () => "work-a", getScopeKey: () => "local/work-a",
    persistTestsOpen: vi.fn(), persistOutputOpen: vi.fn(), persistSelectedTask: vi.fn(), persistRunRefs: vi.fn(),
    prepareRun: async () => true, ensureLease: async () => ({ leaseId: "lease", generation: 1 }),
    onError: vi.fn(), refreshDetail: async () => {},
  });
  tasks.projectTasks = [{ id: "test", label: "Test", kind: "test", argv: ["cargo", "test"], provider: "cargo", root: "." }];
  vi.spyOn(tasks, "runDetected").mockResolvedValue(undefined);
  vi.spyOn(tasks, "stopDetected").mockResolvedValue(undefined);
  return tasks;
}

it("opens Run… without running, and selecting a command requires an explicit run", async () => {
  const tasks = runController();
  component = mount(CodeCommandPicker, { target: document.body, props: { tasks, variant: "run" } }); flushSync();
  const choose = document.querySelector<HTMLButtonElement>('[aria-label="Choose project command"]')!;
  expect(choose.textContent).toContain("Run…"); expect(choose.disabled).toBe(false);
  choose.click(); await vi.waitFor(() => expect(document.querySelector('[data-command]')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('[data-command]')!.click(); flushSync();
  expect(tasks.selectedTaskId).toBe("test"); expect(tasks.runDetected).not.toHaveBeenCalled();
  document.querySelector<HTMLButtonElement>('[aria-label="Run Test"]')!.click(); flushSync();
  expect(tasks.runDetected).toHaveBeenCalledOnce();
});

it("keeps selected commands gated by custody and availability, with Stop still reachable", () => {
  const tasks = runController(); tasks.selectedTaskId = "test";
  component = mount(CodeCommandPicker, { target: document.body, props: { tasks, variant: "run", runDisabled: true, runTitle: "Resume editing first" } }); flushSync();
  const run = document.querySelector<HTMLButtonElement>('[aria-label="Run Test"]')!;
  expect(run.disabled).toBe(true); expect(run.title).toBe("Resume editing first"); run.click(); expect(tasks.runDetected).not.toHaveBeenCalled();
  tasks.running = true; flushSync();
  document.querySelector<HTMLButtonElement>('[aria-label="Stop project command"]')!.click();
  expect(tasks.stopDetected).toHaveBeenCalledOnce();
});

it("does not offer an enabled run when the selected tool is unavailable or saves are pending", () => {
  const tasks = runController(); tasks.selectedTaskId = "test"; tasks.projectTasks[0].available = false;
  component = mount(CodeCommandPicker, { target: document.body, props: { tasks, variant: "run" } }); flushSync();
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Run Test"]')!.disabled).toBe(true);
  tasks.projectTasks[0].available = true; tasks.preparing = true; flushSync();
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Run Test"]')!.disabled).toBe(true);
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Choose project command"]')!.disabled).toBe(true);
});
