import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import {
  parseLiveMarkdown,
  serializeLiveMarkdown,
} from "./liveMarkdownCodec";
import { createLiveExtensions } from "./liveExtensions";

const sample = `---
kind: daily
tags: [a]
---

# Hello

Some **bold** text and a list:

- one
- two

\`\`\`chart
type: bar
title: Visitors
\`\`\`
`;

function mount(md: string) {
  const parsed = parseLiveMarkdown(md);
  const host = document.createElement("div");
  document.body.appendChild(host);
  const editor = new Editor({
    element: host,
    extensions: createLiveExtensions(),
    content: parsed.doc,
  });
  return { editor, host, frontmatter: parsed.frontmatter };
}

describe("VaultLiveEditor TipTap mount", () => {
  it("keeps content after Editor create + serialize", () => {
    const { editor, host, frontmatter } = mount(sample);
    const out = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    expect(editor.getText()).toContain("Hello");
    expect(out).toContain("# Hello");
    expect(out).toContain("```chart");
    editor.destroy();
    host.remove();
  });

  it("does not shrink across markDirty-style reload cycles", () => {
    let md = sample;
    const lengths: number[] = [];
    for (let i = 0; i < 8; i++) {
      const parsed = parseLiveMarkdown(md);
      const host = document.createElement("div");
      document.body.appendChild(host);
      const editor = new Editor({
        element: host,
        extensions: createLiveExtensions(),
        content: parsed.doc,
      });
      md = serializeLiveMarkdown(editor.getJSON(), parsed.frontmatter);
      lengths.push(md.length);
      editor.destroy();
      host.remove();
    }
    expect(Math.min(...lengths)).toBeGreaterThan(40);
    expect(lengths[7]).toBeGreaterThanOrEqual(lengths[0]! - 30);
    expect(md).toContain("Hello");
    expect(md).toContain("```chart");
  });

  it("empty mount serialize must not look like a wiped note with only newline", () => {
    const { editor, host, frontmatter } = mount("");
    const out = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    // empty doc is ok; the host must refuse to markDirty empty over non-empty
    expect(out.trim().length).toBeLessThan(5);
    editor.destroy();
    host.remove();
  });

  it("does not warn about duplicate link extensions", () => {
    const warns: string[] = [];
    const orig = console.warn;
    console.warn = (...args: unknown[]) => {
      warns.push(String(args[0] ?? ""));
    };
    try {
      const { editor, host } = mount("# Hi\n");
      editor.destroy();
      host.remove();
    } finally {
      console.warn = orig;
    }
    expect(warns.some((w) => w.includes("Duplicate extension"))).toBe(false);
  });
});
