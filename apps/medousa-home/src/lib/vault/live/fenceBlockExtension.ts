import { Node, mergeAttributes } from "@tiptap/core";
import {
  detectFenceTitle,
  fencePreviewLine,
  parseFenceInfo,
} from "./fenceCard";

export type FenceBlockAttrs = {
  raw: string;
  lang: string;
  title: string | null;
  preview: string;
};

export type FenceBlockOptions = {
  onEditInBuild?: (attrs: FenceBlockAttrs) => void;
};

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    fenceBlock: {
      insertFenceBlock: (raw: string) => ReturnType;
    };
  }
}

function attrsFromRaw(raw: string): FenceBlockAttrs {
  const open = /^```([^\r\n`]*)\r?\n/.exec(raw);
  const info = open?.[1] ?? "";
  const { lang } = parseFenceInfo(info);
  const closeIdx = raw.lastIndexOf("\n```");
  const body =
    open && closeIdx > open[0].length
      ? raw.slice(open[0].length, closeIdx)
      : raw.replace(/^```[^\n]*\n?/, "").replace(/\n?```\s*$/, "");
  return {
    raw,
    lang: lang || "code",
    title: detectFenceTitle(body),
    preview: fencePreviewLine(body),
  };
}

export const FenceBlock = Node.create<FenceBlockOptions>({
  name: "fenceBlock",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  addOptions() {
    return {
      onEditInBuild: undefined,
    };
  },

  addAttributes() {
    return {
      raw: { default: "```\n```" },
      lang: { default: "code" },
      title: { default: null },
      preview: { default: "" },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-fence-block]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-fence-block": "" })];
  },

  addCommands() {
    return {
      insertFenceBlock:
        (raw: string) =>
        ({ commands }) => {
          const attrs = attrsFromRaw(raw.trimEnd() + (raw.endsWith("\n") ? "" : "\n"));
          // Ensure trailing newline outside atom via paragraph after insert
          return commands.insertContent([
            { type: this.name, attrs },
            { type: "paragraph" },
          ]);
        },
    };
  },

  addNodeView() {
    return ({ node }) => {
      let attrs = node.attrs as FenceBlockAttrs;
      const dom = document.createElement("div");
      dom.className = "vault-live-fence-card";
      dom.setAttribute("data-fence-block", "");
      dom.setAttribute("data-lang", attrs.lang || "code");
      dom.contentEditable = "false";

      const head = document.createElement("div");
      head.className = "vault-live-fence-card__head";

      const label = document.createElement("span");
      label.className = "vault-live-fence-card__lang";
      label.textContent = attrs.lang || "code";

      const title = document.createElement("span");
      title.className = "vault-live-fence-card__title";
      title.textContent = attrs.title ?? "";

      head.append(label, title);

      const preview = document.createElement("p");
      preview.className = "vault-live-fence-card__preview";
      preview.textContent = attrs.preview || "Fenced block";

      const actions = document.createElement("div");
      actions.className = "vault-live-fence-card__actions";

      const editBtn = document.createElement("button");
      editBtn.type = "button";
      editBtn.className = "vault-live-fence-card__edit";
      editBtn.textContent = "Edit in Build";
      editBtn.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      editBtn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        const opts = this.options as FenceBlockOptions;
        opts.onEditInBuild?.(attrs);
      });

      actions.append(editBtn);
      dom.append(head, preview, actions);

      return {
        dom,
        update: (updated) => {
          if (updated.type.name !== this.name) return false;
          attrs = updated.attrs as FenceBlockAttrs;
          label.textContent = attrs.lang || "code";
          title.textContent = attrs.title ?? "";
          preview.textContent = attrs.preview || "Fenced block";
          dom.setAttribute("data-lang", attrs.lang || "code");
          return true;
        },
      };
    };
  },
});

export { attrsFromRaw as fenceAttrsFromRaw };
