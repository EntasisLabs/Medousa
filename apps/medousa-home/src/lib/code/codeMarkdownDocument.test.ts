/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
const fetch = vi.hoisted(() => vi.fn());
vi.mock("$lib/forge", () => ({ forgeFetch: fetch }));
vi.mock("$lib/daemon", () => ({ operationPath: (_operation: string, args: { work_id: string }) => `/v1/forge/items/${args.work_id}/source` }));
import { projectMarkdownTarget, ProjectMarkdownImages } from "./codeMarkdownDocument";

describe("project Markdown context", () => {
  it("resolves relative and root links, anchors, encoded names, and refuses escapes", () => {
    expect(projectMarkdownTarget("docs/readme.md", "../images/a%20b.png")).toEqual({ path: "images/a b.png", hash: "" });
    expect(projectMarkdownTarget("docs/readme.md", "/README.md#hello")).toEqual({ path: "README.md", hash: "hello" });
    expect(projectMarkdownTarget("docs/readme.md", "#hello")).toEqual({ path: "docs/readme.md", hash: "hello" });
    for (const href of ["../../secret", "../.git/config", "file:///etc/passwd", "https://example.com", "//example.com", "x%5Cy", "x%00y"]) expect(projectMarkdownTarget("docs/readme.md", href)).toBeNull();
  });
  it("uses the correct undertaking and releases resolved image URLs", async () => {
    fetch.mockResolvedValue({ mime: "image/png", bytes_base64: "eA==" });
    const create = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:project-image");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const images = new ProjectMarkdownImages("work-two", "docs/readme.md");
    expect(await images.resolve("../image.png")).toBe("blob:project-image");
    expect(await images.resolve("../image.png")).toBe("blob:project-image");
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith("/v1/forge/items/work-two/source?path=image.png&image=true");
    images.dispose();
    expect(revoke).toHaveBeenCalledWith("blob:project-image");
    create.mockRestore(); revoke.mockRestore();
  });
  it("does not retain bytes from a request completing after the document closes", async () => {
    let resolve!: (value: unknown) => void;
    fetch.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const images = new ProjectMarkdownImages("other", "README.md");
    const pending = images.resolve("image.png");
    images.dispose();
    resolve({ mime: "image/png", bytes_base64: "eA==" });
    await expect(pending).rejects.toThrow("Document closed");
  });
});
