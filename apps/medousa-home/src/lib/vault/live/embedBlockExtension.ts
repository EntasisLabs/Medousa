import { Node, mergeAttributes } from "@tiptap/core";
import { getVaultNote, saveVaultNote } from "$lib/daemon";
import { vaultDisplayTitle } from "$lib/utils/formatVault";
import {
  serializeFrontmatter,
  stripFrontmatter,
} from "$lib/utils/vaultFrontmatter";
import { resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
import {
  invalidateTransclusionCache,
  resolveTransclusions,
} from "$lib/utils/resolveTransclusion";
import {
  destroyLiquidEmbeds,
  hydrateLiquidEmbeds,
} from "$lib/markdown/hydrateLiquidEmbeds";
import type { LiquidRenderContext } from "$lib/liquid/render/context";
import type { VaultNote } from "$lib/types/vault";
import { pushForeignUndo } from "./liveForeignUndo";
import {
  LIVE_ICON_CHEVRON_DOWN,
  LIVE_ICON_CHEVRON_UP,
  LIVE_ICON_DETACH,
  LIVE_ICON_OPEN,
} from "./liveIcons";
import { liveNodeViewStopEvent } from "./liveNodeViewStopEvent";

export type EmbedBlockAttrs = {
  path: string;
  label: string;
};

export type LiveEmbedResolveContext = {
  sourcePath: string | null;
  notes: VaultNote[];
  selectedPath: string | null;
  selectedContent: string;
  labelByPath: Map<string, string>;
};

export type EmbedBlockOptions = {
  getResolveContext?: () => LiveEmbedResolveContext;
  getLiquidContext?: () => LiquidRenderContext;
  onOpenNote?: (path: string) => void;
  /** Convert embed atom into a prose wikilink (reshape). */
  onDetach?: (path: string, label: string, pos: number) => void;
  /** When write-through targets the open note, update vault.content. */
  onWriteThroughSelected?: (path: string, content: string) => void;
};

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    embedBlock: {
      insertEmbedBlock: (path: string, label?: string) => ReturnType;
    };
  }
}

export function embedAttrsFromPath(path: string, label?: string): EmbedBlockAttrs {
  const clean = path.replace(/^!\[\[|\]\]$/g, "").trim();
  const token = clean.replace(/\.md$/i, "");
  return {
    path: clean,
    label: label?.trim() || vaultDisplayTitle(token.split("/").pop() ?? token, clean),
  };
}

async function loadEmbedMarkdown(
  resolvedPath: string,
  ctx: LiveEmbedResolveContext,
): Promise<{ full: string; body: string; frontmatter: string | null } | null> {
  let full: string | null = null;
  if (resolvedPath === ctx.selectedPath) {
    full = ctx.selectedContent;
  } else {
    try {
      const res = await getVaultNote(resolvedPath);
      full = res.content;
    } catch {
      return null;
    }
  }
  if (full == null) return null;
  const stripped = stripFrontmatter(full);
  return {
    full,
    body: stripped.content,
    frontmatter: stripped.frontmatter,
  };
}

function reattachFrontmatter(frontmatter: string | null, body: string): string {
  if (frontmatter == null) {
    return body.endsWith("\n") ? body : `${body}\n`;
  }
  return serializeFrontmatter(frontmatter, body);
}

export const EmbedBlock = Node.create<EmbedBlockOptions>({
  name: "embedBlock",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  addOptions() {
    return {
      getResolveContext: undefined,
      getLiquidContext: undefined,
      onOpenNote: undefined,
      onDetach: undefined,
      onWriteThroughSelected: undefined,
    };
  },

  addAttributes() {
    return {
      path: { default: "" },
      label: { default: "" },
    };
  },

  parseHTML() {
    return [{ tag: "div[data-embed-block]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-embed-block": "" })];
  },

  addCommands() {
    return {
      insertEmbedBlock:
        (path: string, label?: string) =>
        ({ commands }) =>
          commands.insertContent([
            { type: this.name, attrs: embedAttrsFromPath(path, label) },
            { type: "paragraph" },
          ]),
    };
  },

  addNodeView() {
    return ({ node, getPos }) => {
      let attrs = node.attrs as EmbedBlockAttrs;
      const dom = document.createElement("div");
      dom.className = "vault-live-organism-host vault-live-embed-host";
      dom.setAttribute("data-embed-block", "");
      dom.dataset.embedPath = attrs.path;
      dom.contentEditable = "false";

      let mountGen = 0;
      let resolvedPath: string | null = null;
      let cachedFrontmatter: string | null = null;
      let editing = false;

      const opts = () => this.options as EmbedBlockOptions;

      const remount = (nextAttrs: EmbedBlockAttrs) => {
        const gen = ++mountGen;
        editing = false;
        const wrap = dom.querySelector(".vault-live-organism");
        if (wrap instanceof HTMLElement) destroyLiquidEmbeds(wrap);
        dom.replaceChildren();
        dom.dataset.embedPath = nextAttrs.path;

        const shell = document.createElement("aside");
        shell.className =
          "markdown-transclusion vault-live-embed vault-live-embed--collapsed vault-live-embed--fold";
        shell.innerHTML = `<header class="markdown-transclusion-header vault-live-embed__header" data-live-embed-header><span class="markdown-transclusion-label"></span><div class="vault-live-embed__actions"><button type="button" class="vault-live-embed__icon-btn vault-live-embed__expand" data-live-embed-expand aria-expanded="false" title="Expand" aria-label="Expand note">${LIVE_ICON_CHEVRON_DOWN}</button><span class="vault-live-quiet-chrome vault-live-embed__secondary"><button type="button" class="vault-live-embed__icon-btn" data-live-embed-open title="Open" aria-label="Open note">${LIVE_ICON_OPEN}</button><button type="button" class="vault-live-embed__icon-btn" data-live-embed-detach title="Remove from page" aria-label="Remove from page">${LIVE_ICON_DETACH}</button></span></div></header><div class="markdown-transclusion-body markdown-content vault-live-embed__body" data-live-embed-body></div>`;
        dom.append(shell);

        const labelEl = shell.querySelector(".markdown-transclusion-label");
        const bodyEl = shell.querySelector<HTMLElement>("[data-live-embed-body]");
        const headerEl = shell.querySelector<HTMLElement>("[data-live-embed-header]");
        if (labelEl) labelEl.textContent = nextAttrs.label || nextAttrs.path;

        const expandBtn = shell.querySelector<HTMLButtonElement>("[data-live-embed-expand]");
        const setExpanded = (expanded: boolean) => {
          shell.classList.toggle("vault-live-embed--collapsed", !expanded);
          shell.classList.toggle("vault-live-embed--expanded", expanded);
          expandBtn?.setAttribute("aria-expanded", expanded ? "true" : "false");
          expandBtn?.setAttribute(
            "title",
            expanded ? "Collapse note" : "Expand note",
          );
          expandBtn?.setAttribute(
            "aria-label",
            expanded ? "Collapse note" : "Expand note",
          );
          if (expandBtn) {
            expandBtn.innerHTML = expanded
              ? LIVE_ICON_CHEVRON_UP
              : LIVE_ICON_CHEVRON_DOWN;
          }
        };
        expandBtn?.addEventListener("mousedown", (e) => {
          e.preventDefault();
          e.stopPropagation();
        });
        expandBtn?.addEventListener("click", (e) => {
          e.preventDefault();
          e.stopPropagation();
          setExpanded(shell.classList.contains("vault-live-embed--collapsed"));
        });

        // Header click toggles open/closed — note opens in the stream, not a ritual.
        headerEl?.addEventListener("click", (e) => {
          const t = e.target as HTMLElement;
          if (t.closest("button")) return;
          e.preventDefault();
          e.stopPropagation();
          setExpanded(shell.classList.contains("vault-live-embed--collapsed"));
        });

        const openBtn = shell.querySelector<HTMLButtonElement>("[data-live-embed-open]");
        openBtn?.addEventListener("mousedown", (e) => {
          e.preventDefault();
          e.stopPropagation();
        });
        openBtn?.addEventListener("click", (e) => {
          e.preventDefault();
          e.stopPropagation();
          if (resolvedPath) opts().onOpenNote?.(resolvedPath);
        });

        const detachBtn = shell.querySelector<HTMLButtonElement>("[data-live-embed-detach]");
        detachBtn?.addEventListener("mousedown", (e) => {
          e.preventDefault();
          e.stopPropagation();
        });
        detachBtn?.addEventListener("click", (e) => {
          e.preventDefault();
          e.stopPropagation();
          const pos = typeof getPos === "function" ? getPos() : null;
          if (typeof pos !== "number") return;
          opts().onDetach?.(nextAttrs.path, nextAttrs.label, pos);
        });

        const ctx = opts().getResolveContext?.();
        if (!ctx || !bodyEl) {
          if (bodyEl) bodyEl.textContent = "Embed unavailable";
          return;
        }

        const token = nextAttrs.path.replace(/\.md$/i, "");
        const path =
          resolveWikilinkTarget(token, ctx.sourcePath, ctx.notes) ??
          resolveWikilinkTarget(nextAttrs.path, ctx.sourcePath, ctx.notes);
        if (!path) {
          bodyEl.textContent = `Note not found: ${nextAttrs.path}`;
          return;
        }
        resolvedPath = path;
        shell.setAttribute("data-transclude-path", path);

        void (async () => {
          const html = await resolveTransclusions(`![[${nextAttrs.path}]]`, ctx);
          if (gen !== mountGen || !bodyEl) return;
          // Pull body from resolved aside if present; else show raw html
          const tmp = document.createElement("div");
          tmp.innerHTML = html;
          const resolvedBody = tmp.querySelector(".markdown-transclusion-body");
          const resolvedLabel = tmp.querySelector(".markdown-transclusion-label");
          if (resolvedLabel && labelEl) {
            labelEl.textContent = resolvedLabel.textContent || nextAttrs.label;
          }
          bodyEl.innerHTML = resolvedBody?.innerHTML ?? html;
          hydrateLiquidEmbeds(bodyEl, {
            context: opts().getLiquidContext?.() ?? {},
            animate: false,
          });

          const loaded = await loadEmbedMarkdown(path, ctx);
          if (gen !== mountGen || !loaded) return;
          cachedFrontmatter = loaded.frontmatter;

          const enterEdit = async () => {
            if (editing || gen !== mountGen) return;
            editing = true;
            setExpanded(true);
            destroyLiquidEmbeds(bodyEl);
            const latest = await loadEmbedMarkdown(path, {
              ...ctx,
              selectedContent:
                path === ctx.selectedPath ? ctx.selectedContent : ctx.selectedContent,
            });
            if (gen !== mountGen || !latest) {
              editing = false;
              return;
            }
            cachedFrontmatter = latest.frontmatter;
            bodyEl.replaceChildren();
            bodyEl.contentEditable = "true";
            bodyEl.classList.add("vault-live-embed__body--editing");
            bodyEl.textContent = latest.body;
            bodyEl.focus();

            const finish = () => {
              if (!editing || gen !== mountGen) return;
              void (async () => {
                const nextBody = bodyEl.textContent?.replace(/\u00a0/g, " ") ?? "";
                const previous = latest.full;
                const nextFull = reattachFrontmatter(cachedFrontmatter, nextBody);
                editing = false;
                bodyEl.contentEditable = "false";
                bodyEl.classList.remove("vault-live-embed__body--editing");
                if (nextFull !== previous) {
                  pushForeignUndo(path, previous);
                  try {
                    if (path === ctx.selectedPath) {
                      opts().onWriteThroughSelected?.(path, nextFull);
                    } else {
                      await saveVaultNote(path, nextFull);
                      invalidateTransclusionCache(path);
                    }
                  } catch {
                    // keep previous rendered state on failure
                  }
                }
                // Re-render from saved content
                const again = await resolveTransclusions(`![[${nextAttrs.path}]]`, {
                  ...ctx,
                  selectedContent:
                    path === ctx.selectedPath ? nextFull : ctx.selectedContent,
                });
                if (gen !== mountGen) return;
                const t2 = document.createElement("div");
                t2.innerHTML = again;
                const b2 = t2.querySelector(".markdown-transclusion-body");
                bodyEl.innerHTML = b2?.innerHTML ?? again;
                hydrateLiquidEmbeds(bodyEl, {
                  context: opts().getLiquidContext?.() ?? {},
                  animate: false,
                });
              })();
            };
            bodyEl.addEventListener("blur", finish, { once: true });
          };

          // Click into the body to edit — no dblclick ritual.
          bodyEl.addEventListener("click", (e) => {
            if (editing) return;
            if ((e.target as HTMLElement).closest("a, button")) return;
            e.preventDefault();
            e.stopPropagation();
            void enterEdit();
          });
        })();
      };

      remount(attrs);

      return {
        dom,
        stopEvent: liveNodeViewStopEvent,
        ignoreMutation: () => true,
        update: (updated) => {
          if (updated.type.name !== this.name) return false;
          const next = updated.attrs as EmbedBlockAttrs;
          if (next.path === attrs.path && next.label === attrs.label) {
            attrs = next;
            return true;
          }
          attrs = next;
          remount(attrs);
          return true;
        },
        destroy: () => {
          mountGen += 1;
          const wrap = dom.querySelector(".vault-live-embed__body");
          if (wrap instanceof HTMLElement) destroyLiquidEmbeds(wrap);
          destroyLiquidEmbeds(dom);
        },
      };
    };
  },
});
