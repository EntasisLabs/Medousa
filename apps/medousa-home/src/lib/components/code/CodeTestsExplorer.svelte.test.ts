/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import CodeTestsExplorer from "./CodeTestsExplorer.svelte";
import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });
it("bounds a 2000-test catalog and makes file targeting explicit", () => {
  const runDetected = vi.fn();
  const tests = Array.from({ length: 2000 }, (_, i) => ({ id: `test-${i}`, label: `case-${i}`, path: `app/file-${i}.ts`, line: 1, task_id: "web", provider: "npm", target_kind: "file" }));
  const tasks = { projectTests: tests, projectTasks: [{ id: "web", root: "app" }], recentRuns: [], testsLoaded: true, runDetected } as unknown as CodeTasksController;
  component = mount(CodeTestsExplorer, { target: document.body, props: { tasks, activePath: "app/file-1999.ts", onOpenLocation: vi.fn() } }); flushSync();
  expect(document.querySelectorAll("details")).toHaveLength(120);
  const input = document.querySelector<HTMLInputElement>('[aria-label="Filter tests"]')!;
  input.value = "case-1999"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(document.querySelectorAll("details")).toHaveLength(1);
  const run = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === "Run file")!;
  run.click(); expect(runDetected).toHaveBeenCalledWith(tests[1999]);
  expect(document.body.textContent).toContain("not verification of current edits");
});
