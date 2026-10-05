/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { createLiveExtensions } from "$lib/vault/live/liveExtensions";
import { MarkdownSourceDocument } from "./markdownSourceDocument";
import { MarkdownSourceBlock, MarkdownSourceIdentity } from "./markdownSourceExtensions";
let editor: Editor | undefined;
afterEach(() => { editor?.destroy(); editor = undefined; });
function open(source: string) {
  const document = new MarkdownSourceDocument(source);
  editor = new Editor({ extensions: [...createLiveExtensions({ trailingNode: false, headingLevels: [1, 2, 3, 4, 5, 6] }), MarkdownSourceIdentity, MarkdownSourceBlock], content: document.doc });
  document.establishBaseline(editor.getJSON());
  return document;
}
describe("repository Markdown source preservation", () => {
  it.each(["", "\n\n", "# Hello\n", "---\r\ntitle:  'Untouched'\r\n---\r\n\r\n## Hello\r\n\r\n*one*  and __two__\r\n", "<section class='custom'>\nHello\n</section>\n\n[reference]: ./somewhere.md\n\nSee [here][reference].\n", "```rust\nfn main() {}\n```\n\n```chart\ntitle: Chart\n```\n"])("opening and leaving preserves exact source: %s", (source) => {
    const doc = open(source);
    expect(doc.serialize(editor!.getJSON())).toBe(source);
  });
  it("edits prose while leaving frontmatter, HTML, definitions, fences and spacing untouched", () => {
    const source = "---\r\ntitle: 'Exact'\r\n---\r\n\r\nHello world.\r\n\r\n<section data-a='x'>raw</section>\r\n\r\n[ref]: ./target.md\r\n\r\n```ts\r\nconst x = 1;\r\n```\r\n";
    const doc = open(source);
    editor!.commands.insertContentAt(7, "brave ");
    expect(doc.serialize(editor!.getJSON())).toBe(source.replace("Hello world", "Hello brave world"));
    editor!.commands.undo();
    expect(doc.serialize(editor!.getJSON())).toBe(source);
  });
  it("keeps reference links as explicit source blocks without losing the definition", () => {
    const doc = open("See [link][ref].\n\n[ref]: ../readme.md\n");
    expect(editor!.getJSON().content?.filter((node) => node.attrs?.sourceBlockId != null).every((node) => node.type === "markdownSourceBlock")).toBe(true);
    expect(doc.serialize(editor!.getJSON())).toBe("See [link][ref].\n\n[ref]: ../readme.md\n");
  });
  it("serializes a new empty-file draft and undo returns to byte-for-byte empty", () => {
    const doc = open("");
    editor!.commands.insertContent("hello");
    expect(doc.serialize(editor!.getJSON()).trim()).toBe("hello");
    editor!.commands.undo();
    expect(doc.serialize(editor!.getJSON())).toBe("");
  });

  it("retains a nested organism fence as one whole source-backed block", () => {
    const raw = "```report\ntitle: Report\n\n```chart\ntitle: Chart\n```\n\n```\n";
    const doc = open("Hello.\n\n" + raw);
    expect(editor!.getJSON().content?.filter((node) => node.type === "fenceBlock")).toHaveLength(1);
    editor!.commands.insertContentAt(1, "Updated ");
    expect(doc.serialize(editor!.getJSON())).toBe("Updated Hello.\n\n" + raw);
  });

  it("retains reordered blocks without merging paragraphs or discarding the change", () => {
    const doc = open("First.\n\nSecond.\n");
    const reordered = editor!.getJSON();
    reordered.content!.reverse();
    expect(doc.serialize(reordered)).toContain("Second.\n\nFirst.");
  });
});
