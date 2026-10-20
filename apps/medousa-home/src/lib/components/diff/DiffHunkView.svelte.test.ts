/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import DiffHunkView from "./DiffHunkView.svelte";
import type { DiffHunk } from "$lib/diff/diffTypes";
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });
const hunks: DiffHunk[] = [{ old_start: 3, old_count: 2, new_start: 3, new_count: 2, lines: [
  { kind: "context", old_line: 3, new_line: 3, content: "context" },
  { kind: "deletion", old_line: 4, content: "old" },
  { kind: "addition", new_line: 4, content: "new" },
] }];
it("keeps old/new comment anchors with one inline number column and no duplicate range bar", () => {
  const onComment = vi.fn();
  component = mount(DiffHunkView, { target: document.body, props: { hunks, continuous: true, onComment } }); flushSync();
  expect(document.querySelector(".diff-hunk-meta")).toBeNull();
  expect(document.querySelectorAll(".diff-line .diff-gutter")).toHaveLength(3);
  document.querySelector<HTMLButtonElement>(".diff-line--deletion button")!.click();
  expect(onComment).toHaveBeenLastCalledWith({ side: "old", line: 4, content: "old" });
  document.querySelector<HTMLButtonElement>(".diff-line--addition button")!.click();
  expect(onComment).toHaveBeenLastCalledWith({ side: "new", line: 4, content: "new" });
});
it("labels omitted patch context without offering nonfunctional expansion", () => {
  component = mount(DiffHunkView, { target: document.body, props: { hunks, continuous: true } }); flushSync();
  expect(document.querySelector(".diff-gap")?.textContent).toContain("2 unmodified lines");
  expect(document.querySelector(".diff-gap button")).toBeNull();
});
it("expands real source context and can collapse it again", () => {
  component = mount(DiffHunkView, { target: document.body, props: { hunks, continuous: true, beforeText: "first\nsecond\ncontext\nold", afterText: "first\nsecond\ncontext\nnew" } }); flushSync();
  document.querySelector<HTMLButtonElement>(".diff-gap-action--all")!.click(); flushSync();
  expect(document.querySelectorAll(".diff-line")).toHaveLength(5);
  expect(document.body.textContent).toContain("first");
  document.querySelector<HTMLButtonElement>(".diff-gap-action")!.click(); flushSync();
  expect(document.querySelectorAll(".diff-line")).toHaveLength(3);
});
