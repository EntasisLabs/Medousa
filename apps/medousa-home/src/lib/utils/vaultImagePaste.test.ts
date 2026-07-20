import { describe, expect, it } from "vitest";
import {
  altFromImageFile,
  formatInlineImageMarkdown,
  markdownFromImageFile,
  VAULT_INLINE_IMAGE_MAX_BYTES,
} from "./vaultImagePaste";

describe("vaultImagePaste", () => {
  it("formats a data-URI markdown image", () => {
    const md = formatInlineImageMarkdown("data:image/png;base64,abc123", "shot");
    expect(md).toContain("![shot](data:image/png;base64,abc123)");
  });

  it("derives alt text from the file name", () => {
    expect(altFromImageFile(new File([], "diagram.png", { type: "image/png" }))).toBe(
      "diagram",
    );
  });

  it("rejects non-image files", async () => {
    const result = await markdownFromImageFile(
      new File(["hi"], "note.txt", { type: "text/plain" }),
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toBe("no-image");
  });

  it("encodes a small image file as markdown", async () => {
    const bytes = new Uint8Array([137, 80, 78, 71]);
    const file = new File([bytes], "pixel.png", { type: "image/png" });
    const result = await markdownFromImageFile(file);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.markdown).toMatch(/^!\[pixel\]\(data:image\/png;base64,/);
      expect(result.byteLength).toBe(4);
    }
  });

  it("rejects images over the soft size cap", async () => {
    const big = new Uint8Array(VAULT_INLINE_IMAGE_MAX_BYTES + 1);
    const file = new File([big], "huge.png", { type: "image/png" });
    const result = await markdownFromImageFile(file);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toBe("too-large");
  });
});
