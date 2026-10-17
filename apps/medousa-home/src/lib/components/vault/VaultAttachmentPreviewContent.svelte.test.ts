/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const fixture = vi.hoisted(() => ({ preview: vi.fn() }));
vi.mock("$lib/utils/vaultAttachmentPicker", () => ({ attachmentPreviewUrl: fixture.preview, openAttachmentPath: vi.fn() }));
vi.mock("$lib/utils/spreadsheetPreviewLoader", () => ({ loadSpreadsheetPreview: vi.fn() }));
import Preview from "./VaultAttachmentPreviewContent.svelte";
let component: ReturnType<typeof mount> | null = null;
afterEach(() => { if (component) unmount(component); component = null; document.body.innerHTML = ""; vi.restoreAllMocks(); });
describe("file preview delivery", () => {
  it("discards a late preview and releases URLs when switching files and closing", async () => {
    const resolvers = new Map<string, (url: string) => void>();
    fixture.preview.mockImplementation((path: string) => new Promise<string>((resolve) => resolvers.set(path, resolve)));
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const props = $state({ attachment: { path: "/pinned/first.png", label: "First" } });
    component = mount(Preview, { target: document.body, props }); flushSync();
    props.attachment = { path: "/pinned/second.png", label: "Second" }; flushSync();
    resolvers.get("/pinned/second.png")!("blob:second"); await Promise.resolve(); flushSync();
    resolvers.get("/pinned/first.png")!("blob:first"); await Promise.resolve(); flushSync();
    expect(document.querySelector("img")?.getAttribute("src")).toBe("blob:second");
    expect(revoke).toHaveBeenCalledWith("blob:first");
    unmount(component); component = null;
    expect(revoke).toHaveBeenCalledWith("blob:second");
  });
});
