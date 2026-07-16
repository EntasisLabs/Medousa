import StarterKit from "@tiptap/starter-kit";
import Link from "@tiptap/extension-link";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Placeholder from "@tiptap/extension-placeholder";
import { Markdown } from "@tiptap/markdown";
import type { AnyExtension } from "@tiptap/core";
import { FenceBlock, type FenceBlockOptions } from "./fenceBlockExtension";

export function createLiveExtensions(
  fenceOptions?: FenceBlockOptions,
): AnyExtension[] {
  return [
    StarterKit.configure({
      // All fences are atomic cards; no editable code blocks in Live.
      codeBlock: false,
      // Use standalone Link below (StarterKit's link conflicts if both are enabled).
      link: false,
      // Horizontal rule from StarterKit; keep heading levels 1–3 for slash.
      heading: { levels: [1, 2, 3] },
    }),
    Link.configure({
      openOnClick: false,
      autolink: true,
      linkOnPaste: true,
    }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Placeholder.configure({
      placeholder: "Write…",
    }),
    Markdown.configure({
      indentation: { style: "space", size: 2 },
    }),
    FenceBlock.configure(fenceOptions ?? {}),
  ];
}
