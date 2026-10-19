/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import ReviewFileChanges from "./ReviewFileChanges.svelte";
import type { ReviewFileDiff, ReviewProjection } from "$lib/forge";

const api = vi.hoisted(() => ({ file: vi.fn() }));
vi.mock("$lib/forge", () => ({ getReviewFile: api.file, getWorldAtLocation: vi.fn() }));

let component: ReturnType<typeof mount> | undefined;
function comparison(path: string): ReviewFileDiff {
  return {
    work_id: "work", attempt_id: "attempt", path, status: "modified",
    baseline_oid: "before", reviewed_oid: "after", binary: true,
    baseline: { exists: true, binary: true, byte_size: 1 },
    reviewed: { exists: true, binary: true, byte_size: 2 },
    hunks: [], changed_lines: [], truncated: false,
  };
}
function review(count: number) {
  return {
    work_id: "work", attempt_id: "attempt", evidence_id: "evidence",
    changed_files: Array.from({ length: count }, (_, i) => ({ path: `file-${String(i).padStart(2, "0")}`, status: "modified" })),
  } as ReviewProjection;
}
async function settle() { for (let i = 0; i < 8; i++) { flushSync(); await tick(); } }
function buttons() { return [...document.querySelectorAll<HTMLButtonElement>(".file-expand")]; }
beforeEach(() => { api.file.mockReset(); api.file.mockImplementation(async (_work, path) => comparison(path)); });
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

it("collapses an evicted comparison and reloads it when reopened", async () => {
  component = mount(ReviewFileChanges, { target: document.body, props: { review: review(13), onOpenFile: vi.fn() } });
  await settle();
  for (const button of buttons()) { button.click(); await settle(); }
  expect(buttons()[0].getAttribute("aria-expanded")).toBe("false");
  expect(buttons()[12].getAttribute("aria-expanded")).toBe("true");
  buttons()[0].click(); await settle();
  expect(api.file.mock.calls.filter((call) => call[1] === "file-00")).toHaveLength(2);
});

it("admits only two file fetches and skips a queued file closed before admission", async () => {
  const finish: Array<() => void> = [];
  api.file.mockImplementation((_work, path) => new Promise((resolve) => finish.push(() => resolve(comparison(path)))));
  component = mount(ReviewFileChanges, { target: document.body, props: { review: review(4), onOpenFile: vi.fn() } });
  await settle();
  for (const button of buttons()) button.click();
  await settle();
  expect(api.file).toHaveBeenCalledTimes(2);
  buttons()[2].click(); await settle();
  finish.shift()!(); await settle();
  expect(api.file).toHaveBeenCalledTimes(3);
  expect(api.file.mock.calls.map((call) => call[1])).toEqual(["file-00", "file-01", "file-03"]);
  for (const done of finish) done();
  await settle();
});

it("stops admitting queued file loads when the review is closed", async () => {
  const finish: Array<() => void> = [];
  api.file.mockImplementation((_work, path) => new Promise((resolve) => finish.push(() => resolve(comparison(path)))));
  component = mount(ReviewFileChanges, { target: document.body, props: { review: review(4), onOpenFile: vi.fn() } });
  await settle();
  for (const button of buttons()) button.click();
  await settle();
  await unmount(component); component = undefined;
  for (const done of finish) done();
  await settle();
  expect(api.file).toHaveBeenCalledTimes(2);
});


it("shows only the selected directory, including nested files but not similarly named siblings", async () => {
  const projection = review(0);
  projection.changed_files = ["README.md", "src/main.rs", "src/nested/lib.rs", "src-other/main.rs", "docs/README.md"].map((path) => ({ path, status: "modified", is_binary: false }));
  component = mount(ReviewFileChanges, { target: document.body, props: { review: projection, directory: "src", onOpenFile: vi.fn() } });
  await settle();
  expect(buttons().map((button) => button.textContent?.trim())).toEqual(["main.rs src", "lib.rs src/nested"]);
  buttons()[0].click(); await settle();
  expect(api.file).toHaveBeenCalledWith("work", "src/main.rs", "attempt");
  expect(api.file).toHaveBeenCalledTimes(1);
});

it("limits Repository root to files outside directories", async () => {
  const projection = review(0);
  projection.changed_files = ["README.md", "src/main.rs", "docs/README.md"].map((path) => ({ path, status: "modified", is_binary: false }));
  component = mount(ReviewFileChanges, { target: document.body, props: { review: projection, directory: "", onOpenFile: vi.fn() } });
  await settle();
  expect(buttons().map((button) => button.textContent?.trim())).toEqual(["README.md"]);
});
