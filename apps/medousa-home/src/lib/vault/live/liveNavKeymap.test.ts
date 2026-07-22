/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";
import { createLiveExtensions } from "./liveExtensions";
import { handleLiveNavKey } from "./liveNavKeymap";

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
          type: "paragraph",
          content: [{ type: "text", text: "  hello world" }],
        },
        {
          type: "paragraph",
          content: [{ type: "text", text: "second" }],
        },
      ],
    },
  });
  return { editor, host };
}

function key(
  name: string,
  opts: Partial<KeyboardEventInit> = {},
): KeyboardEvent {
  return new KeyboardEvent("keydown", {
    key: name,
    bubbles: true,
    cancelable: true,
    ...opts,
  });
}

describe("handleLiveNavKey", () => {
  it("Home uses smart indent then absolute start", () => {
    const { editor, host } = mountDoc();
    // After two spaces: position 3 (doc: 0 doc, 1 p open, 2 first char…)
    // TipTap: first text char is at pos 1 inside first paragraph → "  hello" starts at 1
    editor.commands.setTextSelection(5); // inside "hello"
    expect(handleLiveNavKey(editor, key("Home"))).toBe(true);
    const soft = editor.state.selection.from;
    expect(handleLiveNavKey(editor, key("Home"))).toBe(true);
    const hard = editor.state.selection.from;
    expect(hard).toBeLessThan(soft);
    editor.destroy();
    host.remove();
  });

  it("End moves to end of textblock", () => {
    const { editor, host } = mountDoc();
    editor.commands.setTextSelection(1);
    expect(handleLiveNavKey(editor, key("End"))).toBe(true);
    const text = editor.state.selection.$from.parent.textContent;
    expect(editor.state.selection.from).toBe(
      editor.state.selection.$from.start() + text.length,
    );
    editor.destroy();
    host.remove();
  });

  it("Mod-Home goes to document start", () => {
    const { editor, host } = mountDoc();
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    expect(
      handleLiveNavKey(editor, key("Home", { metaKey: true })),
    ).toBe(true);
    expect(editor.state.selection.from).toBe(1);
    editor.destroy();
    host.remove();
  });

  it("Mod-End goes to document end", () => {
    const { editor, host } = mountDoc();
    editor.commands.setTextSelection(1);
    expect(handleLiveNavKey(editor, key("End", { metaKey: true }))).toBe(true);
    expect(editor.state.selection.from).toBe(
      TextSelection.atEnd(editor.state.doc).from,
    );
    editor.destroy();
    host.remove();
  });
});
