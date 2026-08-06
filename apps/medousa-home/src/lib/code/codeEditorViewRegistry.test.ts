import type { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { codeEditorViewRegistry } from "./codeEditorViewRegistry";

function view(focused = false): EditorView {
  return { hasFocus: focused } as EditorView;
}

describe("code editor view registry", () => {
  it("prefers a focused split and falls back to the newest live view", () => {
    const older = view(true);
    const newer = view(false);
    const unregisterOlder = codeEditorViewRegistry.register(
      "file:///repo/preferred.ts",
      older,
    );
    const unregisterNewer = codeEditorViewRegistry.register(
      "file:///repo/preferred.ts",
      newer,
    );

    expect(codeEditorViewRegistry.get("FILE:///repo/preferred.ts#symbol")).toBe(
      older,
    );
    unregisterOlder();
    expect(codeEditorViewRegistry.get("file:///repo/preferred.ts")).toBe(newer);
    unregisterNewer();
    expect(codeEditorViewRegistry.get("file:///repo/preferred.ts")).toBe(null);
  });

  it("resolves a pending navigation when the target editor mounts", async () => {
    const pending = codeEditorViewRegistry.waitFor(
      "file:///repo/pending.ts",
      100,
    );
    const mounted = view();
    const unregister = codeEditorViewRegistry.register(
      "file:///repo/pending.ts",
      mounted,
    );

    await expect(pending).resolves.toBe(mounted);
    unregister();
  });

  it("bounds a wait for an editor that never mounts", async () => {
    await expect(
      codeEditorViewRegistry.waitFor("file:///repo/missing.ts", 1),
    ).resolves.toBe(null);
  });
});
