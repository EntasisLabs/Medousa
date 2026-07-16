import StarterKit from "@tiptap/starter-kit";
import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { Markdown } from "@tiptap/markdown";
import type { AnyExtension } from "@tiptap/core";
import { FenceBlock, type FenceBlockOptions } from "./fenceBlockExtension";
import { EmbedBlock, type EmbedBlockOptions } from "./embedBlockExtension";
import { HeadingMarks } from "./headingMarksExtension";
import { LiveHorizontalRule } from "./liveHorizontalRule";

export type LiveExtensionOptions = {
  fence?: FenceBlockOptions;
  embed?: EmbedBlockOptions;
};

export function createLiveExtensions(
  options: LiveExtensionOptions = {},
): AnyExtension[] {
  return [
    StarterKit.configure({
      // Fences are organism hosts; no editable code blocks in Live.
      codeBlock: false,
      link: false,
      // Custom Live HR: click unrenders to editable ---.
      horizontalRule: false,
      heading: { levels: [1, 2, 3] },
    }),
    LiveHorizontalRule,
    Link.configure({
      openOnClick: false,
      autolink: true,
      linkOnPaste: true,
      protocols: ["http", "https", "mailto", "wikilink"],
      isAllowedUri: (url, ctx) => {
        if (url.startsWith("wikilink:")) return true;
        return ctx.defaultValidate(url);
      },
      HTMLAttributes: {
        class: "vault-live-link",
      },
    }),
    Placeholder.configure({
      placeholder: "Type / to add a chart, callout, or note",
      emptyEditorClass: "is-editor-empty",
      emptyNodeClass: "is-empty",
      showOnlyWhenEditable: true,
      showOnlyCurrent: true,
    }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Markdown.configure({
      indentation: { style: "space", size: 2 },
    }),
    HeadingMarks,
    FenceBlock.configure(options.fence ?? {}),
    EmbedBlock.configure(options.embed ?? {}),
  ];
}
