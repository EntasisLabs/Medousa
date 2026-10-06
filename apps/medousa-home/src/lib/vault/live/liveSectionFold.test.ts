/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { createLiveExtensions } from "./liveExtensions";
import { parseLiveMarkdown, serializeLiveMarkdown } from "./liveMarkdownCodec";
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

function mountList() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const parsed = parseLiveMarkdown(`### Ficus

- Verdict: good indoors
- Strengths: lower light
- Phoenix advantage: warm weather

  sub here
- lkj asd
- what??

### Head-to-Head

Keep this visible.
`);
  const editor = new Editor({ element: host, extensions: createLiveExtensions(), content: parsed.doc });
  const indent = (text: string) => {
    let pos = -1;
    editor.state.doc.descendants((node, start) => {
      if (node.isText && node.text === text) pos = start;
    });
    expect(pos).toBeGreaterThan(0);
    editor.commands.setTextSelection(pos);
    expect(editor.commands.sinkListItem("listItem")).toBe(true);
  };
  indent("lkj asd");
  indent("what??");
  indent("what??");
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
  it("keeps nested fold controls on the item's first text line after indenting", () => {
    const { editor, host } = mountList();
    const items = [...host.querySelectorAll("li")];
    const parent = items.find((item) => item.firstElementChild?.textContent?.includes("Phoenix advantage"))!;
    const nested = items.find((item) => item.firstElementChild?.textContent?.includes("lkj asd"))!;
    for (const item of [parent, nested]) {
      expect(item.firstElementChild?.tagName).toBe("P");
      expect(item.firstElementChild?.querySelector(".vault-live-fold-btn")).not.toBeNull();
      expect(item.querySelector(":scope > .vault-live-fold-btn")).toBeNull();
    }
    expect(parent.querySelector(":scope > ul > li")).toBe(nested);
    expect(nested.querySelector(":scope > ul > li")?.textContent).toContain("what??");
    editor.destroy();
    host.remove();
  });

  it("folds only a nested item's children and preserves parent, siblings, and Markdown", () => {
    const { editor, host } = mountList();
    const before = serializeLiveMarkdown(editor.getJSON(), "");
    const items = [...host.querySelectorAll("li")];
    const parent = items.find((item) => item.textContent?.includes("Phoenix advantage"))!;
    const nested = items.find((item) => item.textContent?.startsWith("lkj asd"))!;
    const button = nested.querySelector<HTMLButtonElement>(".vault-live-fold-btn")!;
    button.click();
    expect(host.querySelector(".vault-live-section-folded")?.tagName).toBe("UL");
    expect(parent.closest(".vault-live-section-folded")).toBeNull();
    expect(nested.closest(".vault-live-section-folded")).toBeNull();
    expect(items[0]!.closest(".vault-live-section-folded")).toBeNull();
    expect(nested.querySelector(":scope > ul")?.classList.contains("vault-live-section-folded")).toBe(true);
    expect(serializeLiveMarkdown(editor.getJSON(), "")).toBe(before);
    nested.querySelector<HTMLButtonElement>(".vault-live-fold-btn")!.click();
    expect(host.querySelector(".vault-live-section-folded")).toBeNull();
    expect(serializeLiveMarkdown(editor.getJSON(), "")).toBe(before);
    editor.destroy();
    host.remove();
  });

  it("can collapse and reopen a parent while remembering a child's fold", () => {
    const { editor, host } = mountList();
    const parent = [...host.querySelectorAll("li")].find((item) => item.textContent?.includes("Phoenix advantage"))!;
    const nested = [...parent.querySelectorAll("li")].find((item) => item.textContent?.startsWith("lkj asd"))!;
    nested.querySelector<HTMLButtonElement>(".vault-live-fold-btn")!.click();
    parent.querySelector<HTMLButtonElement>(".vault-live-fold-btn")!.click();
    expect(parent.closest(".vault-live-section-folded")).toBeNull();
    expect(parent.querySelectorAll(".vault-live-fold-btn")).toHaveLength(1);
    expect(parent.querySelector(":scope > ul")?.classList.contains("vault-live-section-folded")).toBe(true);
    parent.querySelector<HTMLButtonElement>(".vault-live-fold-btn")!.click();
    expect(parent.querySelectorAll(".vault-live-fold-btn")).toHaveLength(2);
    expect(nested.querySelector(":scope > ul")?.classList.contains("vault-live-section-folded")).toBe(true);
    editor.destroy();
    host.remove();
  });

});
