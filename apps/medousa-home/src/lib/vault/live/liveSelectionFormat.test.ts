/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { createLiveExtensions } from "./liveExtensions";
import {
  applyLiveFormatAction,
  liveActiveFormatActions,
  liveSelectionHasText,
} from "./liveSelectionFormat";

function mount(htmlDoc?: object) {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const editor = new Editor({
    element: host,
    extensions: createLiveExtensions(),
    content: htmlDoc ?? {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [{ type: "text", text: "Hello world" }],
        },
      ],
    },
  });
  return { editor, host };
}

describe("liveSelectionFormat", () => {
  it("detects nonempty text selection", () => {
    const { editor, host } = mount();
    editor.commands.setTextSelection({ from: 1, to: 6 });
    expect(liveSelectionHasText(editor)).toBe(true);
    editor.commands.setTextSelection(1);
    expect(liveSelectionHasText(editor)).toBe(false);
    editor.destroy();
    host.remove();
  });

  it("toggles bold on selection", () => {
    const { editor, host } = mount();
    editor.commands.setTextSelection({ from: 1, to: 6 });
    expect(applyLiveFormatAction(editor, "bold")).toBe(true);
    expect(liveActiveFormatActions(editor)).toContain("bold");
    editor.destroy();
    host.remove();
  });

  it("toggles highlight on selection", () => {
    const { editor, host } = mount();
    editor.commands.setTextSelection({ from: 1, to: 6 });
    expect(applyLiveFormatAction(editor, "highlight")).toBe(true);
    expect(liveActiveFormatActions(editor)).toContain("highlight");
    editor.destroy();
    host.remove();
  });

  it("applies bold from a stashed range after selection collapses", () => {
    const { editor, host } = mount();
    editor.commands.setTextSelection({ from: 1, to: 6 });
    const range = { from: 1, to: 6 };
    editor.commands.setTextSelection(1);
    expect(liveSelectionHasText(editor)).toBe(false);
    expect(applyLiveFormatAction(editor, "bold", range)).toBe(true);
    expect(editor.isActive("bold")).toBe(true);
    editor.destroy();
    host.remove();
  });
});
