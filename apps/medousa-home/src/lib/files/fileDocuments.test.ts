import { describe, it, expect, vi, beforeEach } from "vitest";
const fixture = vi.hoisted(() => ({ read: vi.fn(), write: vi.fn(), scope: "local" }));
vi.mock("$lib/utils/externalDeskApi", () => ({ readExternalFile: fixture.read }));
vi.mock("$lib/utils/vaultFilesystem", () => ({ writeAbsoluteTextFile: fixture.write }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => fixture.scope, isCoLocatedWorkshop: () => fixture.scope === "local" }));
import { FileDocuments } from "./fileDocuments.svelte";
import { fileViewKind } from "./fileTypes";
beforeEach(() => { vi.clearAllMocks(); fixture.scope = "local"; fixture.read.mockResolvedValue({ kind: "text", content: "Original", size_bytes: 8 }); fixture.write.mockResolvedValue(undefined); });
describe("file documents", () => {
  it("keeps a draft across view changes and saves to the original file", async () => {
    const files = new FileDocuments(); const doc = await files.open("/tmp/config.json");
    doc.draft = "Updated"; files.release(doc.path);
    expect(await files.open(doc.path)).toBe(doc);
    await files.save(doc.path);
    expect(fixture.write).toHaveBeenCalledWith("/tmp/config.json", "Updated");
    expect(files.dirty(doc.path)).toBe(false);
    files.release(doc.path); expect(files.get(doc.path)).toBeUndefined();
  });
  it("preserves the draft when the file changes outside Medousa", async () => {
    const files = new FileDocuments(); const doc = await files.open("/tmp/readme.md");
    doc.draft = "My draft";
    fixture.read.mockResolvedValue({ kind: "text", content: "External edit" });
    await files.save(doc.path);
    expect(fixture.write).not.toHaveBeenCalled();
    expect(doc.draft).toBe("My draft"); expect(doc.error).toContain("changed on disk");
  });
  it("does not save a local path after switching workshops during the disk check", async () => {
    const files = new FileDocuments(); const doc = await files.open("/tmp/config.txt"); doc.draft = "Changed";
    fixture.read.mockImplementation(async () => { fixture.scope = "remote"; return { kind: "text", content: "Original" }; });
    await files.save(doc.path); expect(fixture.write).not.toHaveBeenCalled();
    expect(doc.error).toContain("workshop");
  });
  it.each([['README.md', 'markdown'], ['config.json', 'text'], ['script.rs', 'text'], ['Dockerfile', 'text'], ['figure.svg', 'image'], ['report.pdf', 'pdf'], ['sheet.csv', 'spreadsheet'], ['deck.pptx', 'unsupported']])("routes %s to %s", (path, kind) => expect(fileViewKind(path)).toBe(kind));
});
