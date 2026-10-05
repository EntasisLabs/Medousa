/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const diagnostics = vi.hoisted(() => vi.fn());
vi.mock("$lib/code/codingEngineClient", () => ({ getAllCodeWorkspaceDiagnostics: diagnostics }));
import { CodeProblemsController } from "$lib/code/codeProblemsController.svelte";
import CodeProblemsPanel from "./CodeProblemsPanel.svelte";
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });

it("keeps diagnostic rows and failure text steady during a retry, with no error notice wall", async () => {
  const problems = new CodeProblemsController({
    getScopeKey: () => "workshop/project", getWorkId: () => "work", getWorkspaceRoot: () => "/repo",
    getDocumentUri: () => "file:///repo/main.rs", getActiveLanguage: () => "rust", getWorkspaceLanguages: () => ["rust"],
    persistPanel: vi.fn(), openProblem: vi.fn(), onError: vi.fn(), syncDocument: vi.fn(),
  });
  problems.setDocumentProblems([{ message: "borrow issue", severity: "warning", line: 1 }]);
  diagnostics.mockRejectedValueOnce(new Error("HTTP 500: internal error"));
  await problems.refresh();
  component = mount(CodeProblemsPanel, { target: document.body, props: { problems } });
  flushSync();
  const row = document.querySelector('[title="main.rs:1:1"]');
  expect(row).not.toBeNull();
  expect(document.body.textContent).toContain("Project analysis unavailable");
  expect(document.body.textContent).not.toContain("HTTP 500");
  let resolve!: (value: unknown) => void;
  diagnostics.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const pending = problems.refresh();
  flushSync();
  expect(document.querySelector('[title="main.rs:1:1"]')).toBe(row);
  expect(document.body.textContent).toContain("Project analysis unavailable");
  expect(document.body.textContent).not.toContain("Loading project problems");
  resolve({ documents: [] });
  await pending;
  flushSync();
  expect(document.body.textContent).not.toContain("Project analysis unavailable");
  expect(document.querySelector('[title="main.rs:1:1"]')).toBe(row);
  problems.dispose();
});
