import { expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ getChangesFile: vi.fn(), pullChanges: vi.fn() }));
vi.mock("$lib/code/codeDocumentService", () => ({
  ...api, isMissingForgeRoute: () => false,
}));
import { CodeChangesController } from "./codeChangesController.svelte";

function setup(ensureLease = vi.fn(async () => ({ leaseId: "lease", generation: 1 }))) {
  let scope = "workshop-a";
  const changes = new CodeChangesController({
    getScopeKey: () => scope, getWorkId: () => "same-id", ensureLease,
    persistOpen: vi.fn(), onError: vi.fn(), onFilesMutated: vi.fn(), refreshDetail: vi.fn(),
    openReview: vi.fn(), getReviewTitle: () => "Project",
  });
  return { changes, switchWorkshop: () => { scope = "workshop-b"; changes.resetForScope(); } };
}

it("does not apply a delayed diff to a new checkout", async () => {
  let resolve!: (value: unknown) => void;
  api.getChangesFile.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const { changes, switchWorkshop } = setup();
  const pending = changes.selectPath("app.ts");
  switchWorkshop();
  resolve({ path: "app.ts", before: "old", after: "old change" });
  await pending;
  expect(changes.fileDiff).toBeNull();
  expect(changes.selectedPath).toBeNull();
  expect(changes.fileLoading).toBe(false);
});

it("does not pull after a workshop change while waiting for editing authority", async () => {
  let resolve!: (value: { leaseId: string; generation: number }) => void;
  const lease = vi.fn(() => new Promise<{ leaseId: string; generation: number }>((done) => { resolve = done; }));
  const { changes, switchWorkshop } = setup(lease);
  const pending = changes.runSync("pull");
  switchWorkshop();
  resolve({ leaseId: "old-lease", generation: 1 });
  await pending;
  expect(api.pullChanges).not.toHaveBeenCalled();
  expect(changes.syncBusy).toBe(false);
});
