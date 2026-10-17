/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const fixture = vi.hoisted(() => ({ read: vi.fn(), load: vi.fn(), renderer: vi.fn() }));
vi.mock("$lib/utils/externalDeskApi", () => ({ readExternalFile: fixture.read }));
vi.mock("$lib/utils/vaultAttachmentPicker", () => ({ openAttachmentPath: vi.fn() }));
vi.mock("./pdfRenderer", () => ({ pdfRenderer: fixture.renderer }));
import FilePdfDocument from "./FilePdfDocument.svelte";
let component: ReturnType<typeof mount> | undefined;
beforeEach(() => {
  vi.clearAllMocks();
  fixture.read.mockResolvedValue({ kind: "base64", content: btoa("%PDF-bytes") });
  fixture.renderer.mockResolvedValue({ getDocument: fixture.load, BinaryDataFactory: class {} });
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.innerHTML = ""; });
async function settle() { for (let i = 0; i < 6; i++) { await Promise.resolve(); flushSync(); } }

it("delivers PDF bytes to the renderer and destroys its worker when closed", async () => {
  const destroy = vi.fn();
  fixture.load.mockReturnValue({ promise: Promise.resolve({ numPages: 3 }), destroy });
  component = mount(FilePdfDocument, { target: document.body, props: { path: "/outside-vault/report.pdf" } });
  flushSync(); await settle();
  expect(document.body.textContent).toContain("Page 1 of 3");
  const options = fixture.load.mock.calls[0][0];
  expect(new TextDecoder().decode(options.data)).toBe("%PDF-bytes");
  expect(options.useWorkerFetch).toBe(false);
  await unmount(component); component = undefined;
  expect(destroy).toHaveBeenCalledOnce();
});

it("does not create a PDF worker after a closed document finishes reading", async () => {
  let resolve!: (payload: object) => void;
  fixture.read.mockReturnValue(new Promise((done) => { resolve = done; }));
  component = mount(FilePdfDocument, { target: document.body, props: { path: "/outside-vault/slow.pdf" } });
  flushSync(); await unmount(component); component = undefined;
  resolve({ kind: "base64", content: btoa("%PDF-bytes") }); await settle();
  expect(fixture.load).not.toHaveBeenCalled();
});
