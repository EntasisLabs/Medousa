import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { Table } from "@tiptap/extension-table";
import { TableRow } from "@tiptap/extension-table-row";
import { TableCell } from "@tiptap/extension-table-cell";
import { TableHeader } from "@tiptap/extension-table-header";
import { Markdown } from "@tiptap/markdown";
import type { AnyExtension } from "@tiptap/core";
import { FenceBlock, type FenceBlockOptions } from "./fenceBlockExtension";
import { EmbedBlock, type EmbedBlockOptions } from "./embedBlockExtension";
import { HeadingMarks } from "./headingMarksExtension";
import { LiveHorizontalRule } from "./liveHorizontalRule";
import { LiveHighlight } from "./liveHighlightMark";
import { LiveTextColor } from "./liveTextColorMark";

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
    LiveHighlight,
    LiveTextColor,
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
    Image.configure({
      inline: false,
      allowBase64: true,
      HTMLAttributes: {
        class: "vault-live-image",
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
    Table.configure({
      resizable: false,
      HTMLAttributes: { class: "vault-live-table" },
    }),
    TableRow,
    TableHeader,
    TableCell,
    Markdown.configure({
      indentation: { style: "space", size: 2 },
    }),
    HeadingMarks,
    FenceBlock.configure(options.fence ?? {}),
    EmbedBlock.configure(options.embed ?? {}),
  ];
}
