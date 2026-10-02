import { expect, it, vi } from "vitest";
const tree = vi.hoisted(() => vi.fn());
vi.mock("$lib/code/codeDocumentService", () => ({ getUndertakingSourceTree: tree }));
import { CodeQuickOpenController } from "./codeQuickOpenController.svelte";

it("rejects an old file index when the same project ID is on a new workshop", async () => {
  let scope = "workshop-a/project";
  let resolve!: (value: unknown) => void;
  tree.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const quick = new CodeQuickOpenController({
    getScopeKey: () => scope, getWorkId: () => "same-id", getLspClient: () => null,
    pathFromUri: () => null, onError: vi.fn(), onShown: vi.fn(), openFile: vi.fn(), revealLine: vi.fn(),
  });
  const pending = quick.show();
  scope = "workshop-b/project";
  quick.resetForScope();
  resolve({ files: [{ path: "old-only.rs" }] });
  await pending;
  expect(quick.files).toEqual([]);
  expect(quick.loading).toBe(false);
  expect(quick.open).toBe(false);
});
