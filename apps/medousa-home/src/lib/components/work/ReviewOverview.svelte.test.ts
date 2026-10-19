/** @vitest-environment happy-dom */
import { expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import ReviewOverview from "./ReviewOverview.svelte";
import type { ReviewProjection } from "$lib/forge";

it("opens each changed area with its actual directory filter, including repository root", async () => {
  const onShowChanges = vi.fn();
  const review = {
    changed_files: ["README.md", "src/main.rs", "src/lib.rs", "docs/guide.md"].map((path) => ({ path })),
    synthesis: { unresolved_issues: [], outcome: "Original task instructions are not a completed-work summary" },
  } as unknown as ReviewProjection;
  const component = mount(ReviewOverview, { target: document.body, props: { review, originalRequest: "Original task instructions are not a completed-work summary", sourceBranch: "main", onShowChanges } });
  try {
    flushSync();
    expect(document.body.textContent).toContain("guide.md");
    expect(document.body.textContent).not.toContain("Original task instructions are not a completed-work summary");
    document.querySelector<HTMLButtonElement>(".overview-view-all")!.click();
    expect(onShowChanges).toHaveBeenLastCalledWith(null);
    const buttons = [...document.querySelectorAll<HTMLButtonElement>(".overview-areas button")];
    buttons.find((button) => button.textContent?.includes("src/"))!.click();
    expect(onShowChanges).toHaveBeenLastCalledWith("src");
    buttons.find((button) => button.textContent?.includes("Repository root"))!.click();
    expect(onShowChanges).toHaveBeenLastCalledWith("");
    buttons.find((button) => button.textContent?.includes("docs/"))!.click();
    expect(onShowChanges).toHaveBeenLastCalledWith("docs");
  } finally { await unmount(component); document.body.replaceChildren(); }
});
