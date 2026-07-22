/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { createLiveExtensions } from "./liveExtensions";
import { handleLiveHeadingKey } from "./headingKeymap";

function mountHeading(level: 1 | 2 | 3) {
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
          attrs: { level },
          content: [{ type: "text", text: "Title" }],
        },
      ],
    },
  });
  // Caret at start of heading text
  editor.commands.setTextSelection(1);
  return { editor, host };
}

function key(
  key: string,
  opts: Partial<KeyboardEvent> = {},
): KeyboardEvent {
  return new KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
    ...opts,
  });
}

describe("handleLiveHeadingKey", () => {
  it("demotes with Backspace at heading start", () => {
    const { editor, host } = mountHeading(2);
    expect(handleLiveHeadingKey(editor, key("Backspace"))).toBe(true);
    expect(editor.getJSON().content?.[0]?.attrs?.level).toBe(1);
    editor.destroy();
    host.remove();
  });

  it("promotes with # at heading start", () => {
    const { editor, host } = mountHeading(1);
    expect(handleLiveHeadingKey(editor, key("#"))).toBe(true);
    expect(editor.getJSON().content?.[0]?.attrs?.level).toBe(2);
    editor.destroy();
    host.remove();
  });

  it("Mod-3 sets heading level 3", () => {
    const { editor, host } = mountHeading(1);
    expect(
      handleLiveHeadingKey(editor, key("3", { metaKey: true })),
    ).toBe(true);
    expect(editor.getJSON().content?.[0]?.type).toBe("heading");
    expect(editor.getJSON().content?.[0]?.attrs?.level).toBe(3);
    editor.destroy();
    host.remove();
  });

  it("Backspace on h1 becomes paragraph", () => {
    const { editor, host } = mountHeading(1);
    expect(handleLiveHeadingKey(editor, key("Backspace"))).toBe(true);
    expect(editor.getJSON().content?.[0]?.type).toBe("paragraph");
    editor.destroy();
    host.remove();
  });
});
