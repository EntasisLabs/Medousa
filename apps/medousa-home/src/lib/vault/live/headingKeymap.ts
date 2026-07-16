import type { Editor } from "@tiptap/core";

function headingContext(editor: Editor): {
  level: number;
  atStart: boolean;
} | null {
  const { $from, empty } = editor.state.selection;
  if (!empty) return null;
  let depth = $from.depth;
  while (depth > 0) {
    const node = $from.node(depth);
    if (node.type.name === "heading") {
      const start = $from.start(depth);
      return {
        level: Number(node.attrs.level ?? 1),
        atStart: $from.pos === start,
      };
    }
    depth -= 1;
  }
  return null;
}

/** Mod-1/2/3, Backspace demote, `#` promote at heading start. */
export function handleLiveHeadingKey(
  editor: Editor,
  event: KeyboardEvent,
): boolean {
  const mod = event.metaKey || event.ctrlKey;

  if (mod && !event.altKey && !event.shiftKey) {
    if (event.key === "1") {
      event.preventDefault();
      return editor.chain().focus().toggleHeading({ level: 1 }).run();
    }
    if (event.key === "2") {
      event.preventDefault();
      return editor.chain().focus().toggleHeading({ level: 2 }).run();
    }
    if (event.key === "3") {
      event.preventDefault();
      return editor.chain().focus().toggleHeading({ level: 3 }).run();
    }
  }

  const ctx = headingContext(editor);
  if (!ctx?.atStart) return false;

  if (event.key === "Backspace" && !mod && !event.altKey) {
    event.preventDefault();
    if (ctx.level <= 1) {
      return editor.chain().focus().setParagraph().run();
    }
    const next = (ctx.level - 1) as 1 | 2;
    return editor.chain().focus().setHeading({ level: next }).run();
  }

  if (event.key === "#" && !mod && !event.altKey) {
    if (ctx.level >= 3) {
      event.preventDefault();
      return true;
    }
    event.preventDefault();
    const next = (ctx.level + 1) as 2 | 3;
    return editor.chain().focus().setHeading({ level: next }).run();
  }

  return false;
}
