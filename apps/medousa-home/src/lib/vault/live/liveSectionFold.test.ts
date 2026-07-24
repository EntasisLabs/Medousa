/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { createLiveExtensions } from "./liveExtensions";
import { foldRangeForTest, sectionFoldEnd } from "./liveSectionFold";

function mountDoc() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const editor = new Editor({
    element: host,
    extensions: createLiveExtensions(),
    content: {
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 1 },
          content: [{ type: "text", text: "One" }],
        },
        {
          type: "paragraph",
          content: [{ type: "text", text: "under h1" }],
        },
        {
          type: "heading",
          attrs: { level: 2 },
          content: [{ type: "text", text: "Two" }],
        },
        {
          type: "paragraph",
          content: [{ type: "text", text: "under h2" }],
        },
        {
          type: "heading",
          attrs: { level: 2 },
          content: [{ type: "text", text: "Two-b" }],
        },
        {
          type: "paragraph",
          content: [{ type: "text", text: "after" }],
        },
      ],
    },
  });
  return { editor, host };
}

describe("liveSectionFold", () => {
  it("folds H2 content until the next H2/H1", () => {
    const { editor, host } = mountDoc();
    let h2Pos = -1;
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === "heading" && node.attrs.level === 2 && h2Pos < 0) {
        h2Pos = pos;
      }
    });
    expect(h2Pos).toBeGreaterThanOrEqual(0);
    const range = foldRangeForTest(editor.state.doc, h2Pos);
    expect(range).not.toBeNull();
    const text = editor.state.doc.textBetween(range!.from, range!.to);
    expect(text).toContain("under h2");
    expect(text).not.toContain("Two-b");
    expect(text).not.toContain("after");
    editor.destroy();
    host.remove();
  });

  it("fold at doc end is a no-op range (empty body)", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const editor = new Editor({
      element: host,
      extensions: createLiveExtensions(),
      content: {
        type: "doc",
        content: [
          {
            type: "heading",
            attrs: { level: 1 },
            content: [{ type: "text", text: "Only" }],
          },
        ],
      },
    });
    const end = sectionFoldEnd(editor.state.doc, 0);
    expect(end).toBe(editor.state.doc.content.size);
    const range = foldRangeForTest(editor.state.doc, 0);
    expect(range).toEqual({ from: end, to: end });
    editor.destroy();
    host.remove();
  });

  it("does not leave fold widgets inside a collapsed section", () => {
    const { editor, host } = mountDoc();
    const buttonsBefore = host.querySelectorAll(".vault-live-fold-btn").length;
    expect(buttonsBefore).toBeGreaterThanOrEqual(3);

    const firstBtn = host.querySelector<HTMLButtonElement>(".vault-live-fold-btn");
    expect(firstBtn).not.toBeNull();
    firstBtn!.click();

    // Nested H2 chevrons must not remain as hit targets under the fold.
    const buttonsAfter = [...host.querySelectorAll(".vault-live-fold-btn")];
    expect(buttonsAfter.length).toBe(1);
    expect(buttonsAfter[0]?.getAttribute("aria-expanded")).toBe("false");

    (buttonsAfter[0] as HTMLButtonElement).click();
    expect(host.querySelectorAll(".vault-live-fold-btn").length).toBe(buttonsBefore);

    editor.destroy();
    host.remove();
  });
});
