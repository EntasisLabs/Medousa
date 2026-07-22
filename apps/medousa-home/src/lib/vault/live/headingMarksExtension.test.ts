/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { createLiveExtensions } from "./liveExtensions";

describe("HeadingMarks hideSyntax", () => {
  it("shows marks when focused and hideSyntax is off", () => {
    let hide = false;
    const host = document.createElement("div");
    document.body.appendChild(host);
    const editor = new Editor({
      element: host,
      extensions: createLiveExtensions({
        hideMarkdownSyntax: () => hide,
      }),
      content: {
        type: "doc",
        content: [
          {
            type: "heading",
            attrs: { level: 2 },
            content: [{ type: "text", text: "Title" }],
          },
        ],
      },
    });
    editor.commands.setTextSelection(1);
    editor.view.dom.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    expect(host.querySelector(".vault-live-heading-marks")).toBeTruthy();
    editor.destroy();
    host.remove();
  });

  it("hides marks when hideSyntax toggle is on", () => {
    let hide = true;
    const host = document.createElement("div");
    document.body.appendChild(host);
    const editor = new Editor({
      element: host,
      extensions: createLiveExtensions({
        hideMarkdownSyntax: () => hide,
      }),
      content: {
        type: "doc",
        content: [
          {
            type: "heading",
            attrs: { level: 2 },
            content: [{ type: "text", text: "Title" }],
          },
        ],
      },
    });
    editor.commands.setTextSelection(1);
    editor.view.dom.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    expect(host.querySelector(".vault-live-heading-marks")).toBeNull();
    editor.destroy();
    host.remove();
  });
});
