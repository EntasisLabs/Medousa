/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import CodeCommandPicker from "./CodeCommandPicker.svelte";
import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
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
