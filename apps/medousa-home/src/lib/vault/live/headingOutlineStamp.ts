import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { uniqueHeadingSlug } from "$lib/utils/headingSlug";

const headingOutlineStampKey = new PluginKey("liveHeadingOutlineStamp");

/**
 * Keep Live TipTap headings aligned with preview outline anchors:
 * `id`, `class="markdown-heading"`, `data-heading-slug`.
 *
 * Batched to one rAF per frame and skips no-op writes so we don't thrash
 * layout while typing.
 */
export const HeadingOutlineStamp = Extension.create({
  name: "liveHeadingOutlineStamp",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: headingOutlineStampKey,
        view(editorView) {
          let raf = 0;
          let lastDoc = editorView.state.doc;

          const stamp = () => {
            raf = 0;
            const counts = new Map<string, number>();
            editorView.state.doc.descendants((node, pos) => {
              if (node.type.name !== "heading") return;
              const level = Number(node.attrs.level ?? 1);
              if (level < 1 || level > 3) return;
              const text = node.textContent.replace(/\s+/g, " ").trim();
              if (!text) return;
              const slug = uniqueHeadingSlug(text, counts);
              const dom = editorView.nodeDOM(pos);
              if (!(dom instanceof HTMLElement)) return;
              if (dom.tagName !== `H${level}`) return;
              if (dom.id !== slug) dom.id = slug;
              if (!dom.classList.contains("markdown-heading")) {
                dom.classList.add("markdown-heading");
              }
              if (dom.getAttribute("data-heading-slug") !== slug) {
                dom.setAttribute("data-heading-slug", slug);
              }
            });
          };

          const schedule = () => {
            if (raf) return;
            raf = requestAnimationFrame(stamp);
          };

          // First paint: stamp synchronously so outline jumps work immediately.
          stamp();
          return {
            update(view) {
              if (view.state.doc.eq(lastDoc)) return;
              lastDoc = view.state.doc;
              schedule();
            },
            destroy() {
              if (raf) cancelAnimationFrame(raf);
            },
          };
        },
      }),
    ];
  },
});
