/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import type { CodeEditorStatusSnapshot } from "$lib/stores/codeEditorStatus.svelte";
import CodeStatusPopover from "./CodeStatusPopover.svelte";
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });

it("keeps technical details collapsed and makes status recovery keyboard accessible", async () => {
  const trigger = document.createElement("button");
  document.body.appendChild(trigger);
  const close = vi.fn();
  const retry = vi.fn();
  const status: CodeEditorStatusSnapshot = {
    workId: "project", path: "main.rs", line: 1, totalLines: 10, column: 1,
    language: "rust", indentation: "Spaces: 4", issueCount: 0, dirty: true,
    saving: false, saveWhisper: "Save blocked", control: "You editing", execution: null,
    languageState: "ready", languageDetail: null, analysis: "unavailable",
    issues: [{ id: "analysis", label: "Analysis unavailable", summary: "Analysis unavailable", guidance: "Retry from Problems.", details: "HTTP 500: internal error" }],
  };
  component = mount(CodeStatusPopover, { target: document.body, props: {
    open: true, triggerEl: trigger, status, onClose: close, onRefreshProblems: retry,
    onShowProblems: vi.fn(), onShowLogs: vi.fn(), onRestart: vi.fn(), onRepair: vi.fn(),
  } });
  flushSync();
  await tick();
  flushSync();
  await tick();
  const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
  expect(dialog).not.toBeNull();
  expect(document.activeElement).toBe(dialog);
  expect(document.querySelector("details")?.open).toBe(false);
  expect(dialog.textContent).toContain("Project analysis is unavailable");
  const retryButton = Array.from(dialog.querySelectorAll("button")).find((button) => button.textContent === "Retry analysis")!;
  retryButton.click();
  expect(retry).toHaveBeenCalledOnce();
  expect(close).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(trigger);
  dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  expect(close).toHaveBeenCalledTimes(2);
});
