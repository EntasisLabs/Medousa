import { Node, mergeAttributes } from "@tiptap/core";
import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  detectFenceTitle,
  fencePreviewLine,
  parseFenceInfo,
} from "./fenceCard";
import {
  isLiquidFenceLang,
  mountLiquidFence,
  mountPlainFence,
  unmountLiquidFence,
} from "./liveOrganismHost";
import { liveNodeViewStopEvent } from "./liveNodeViewStopEvent";
import {
  mountCalloutSurface,
  parseCalloutRaw,
  serializeCalloutRaw,
  type CalloutSurfaceHandles,
} from "./liveCalloutSurface";
import {
  mountReportSurface,
  type ReportSurfaceHandles,
} from "./liveReportSurface";
import {
  mountSlidesSurface,
  type SlidesSurfaceHandles,
} from "./liveSlidesSurface";
import {
  mountChartSurface,
  type ChartSurfaceHandles,
} from "./liveChartSurface";
import {
  mountCardSurface,
  type CardSurfaceHandles,
} from "./liveCardSurface";
import {
  mountDashboardSurface,
  type DashboardSurfaceHandles,
} from "./liveDashboardSurface";
import {
  mountTabsSurface,
  type TabsSurfaceHandles,
} from "./liveTabsSurface";
import {
  mountStepsSurface,
  type StepsSurfaceHandles,
} from "./liveStepsSurface";
import {
  mountAccordionSurface,
  type AccordionSurfaceHandles,
} from "./liveAccordionSurface";
import {
  mountCodeSurface,
  type CodeSurfaceHandles,
} from "./liveCodeSurface";
import {
  mountTreeSurface,
  type TreeSurfaceHandles,
} from "./liveTreeSurface";
import {
  mountCompareSurface,
  type CompareSurfaceHandles,
} from "./liveCompareSurface";
import {
  measureFenceHost,
  mountFenceRawEdit,
  type FenceRawEditHandles,
} from "./fenceRawEdit";
import {
  mountKanbanSurface,
  type KanbanSurfaceHandles,
} from "./liveKanbanSurface";
import { resolveMedousaViews } from "$lib/utils/resolveMedousaViews";
import type { VaultNote } from "$lib/types/vault";

export type FenceBlockAttrs = {
  raw: string;
  lang: string;
  title: string | null;
  preview: string;
};

export type LiveFenceResolveContext = {
  sourcePath: string | null;
  notes: VaultNote[];
  selectedPath: string | null;
  selectedContent: string;
  labelByPath: Map<string, string>;
};

export type FenceBlockOptions = {
  getLiquidContext?: () => LiquidRenderContext;
  getResolveContext?: () => LiveFenceResolveContext;
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

function fenceBody(raw: string): string {
  const open = /^```([^\r\n`]*)\r?\n/.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/, "").replace(/\n?```\s*$/, "");
}

export const FenceBlock = Node.create<FenceBlockOptions>({
  name: "fenceBlock",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  addOptions() {
    return {
      getLiquidContext: undefined,
      getResolveContext: undefined,
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
    return [{ tag: "div[data-fence-block]" }];
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
          return commands.insertContent([
            { type: this.name, attrs },
            { type: "paragraph" },
          ]);
        },
    };
  },

  addNodeView() {
    return ({ node, editor, getPos }) => {
      let attrs = node.attrs as FenceBlockAttrs;
      const dom = document.createElement("div");
      dom.className = "vault-live-organism-host";
      dom.setAttribute("data-fence-block", "");
      dom.setAttribute("data-lang", attrs.lang || "code");
      dom.dataset.liveFenceRaw = attrs.raw;
      dom.contentEditable = "false";

      let callout: CalloutSurfaceHandles | null = null;
      let report: ReportSurfaceHandles | null = null;
      let slides: SlidesSurfaceHandles | null = null;
      let chart: ChartSurfaceHandles | null = null;
      let card: CardSurfaceHandles | null = null;
      let dashboard: DashboardSurfaceHandles | null = null;
      let tabs: TabsSurfaceHandles | null = null;
      let steps: StepsSurfaceHandles | null = null;
      let accordion: AccordionSurfaceHandles | null = null;
      let code: CodeSurfaceHandles | null = null;
      let tree: TreeSurfaceHandles | null = null;
      let compare: CompareSurfaceHandles | null = null;
      let kanban: KanbanSurfaceHandles | null = null;
      let rawEdit: FenceRawEditHandles | null = null;
      let mountGen = 0;

      const applyRawUpdate = (raw: string) => {
        const pos = typeof getPos === "function" ? getPos() : null;
        if (typeof pos !== "number") return;
        const next = attrsFromRaw(raw);
        const tr = editor.state.tr.setNodeMarkup(pos, undefined, next);
        editor.view.dispatch(tr);
      };

      const destroySurfaces = () => {
        rawEdit?.destroy();
        rawEdit = null;
        callout?.destroy();
        callout = null;
        report?.destroy();
        report = null;
        slides?.destroy();
        slides = null;
        chart?.destroy();
        chart = null;
        card?.destroy();
        card = null;
        dashboard?.destroy();
        dashboard = null;
        tabs?.destroy();
        tabs = null;
        steps?.destroy();
        steps = null;
        accordion?.destroy();
        accordion = null;
        code?.destroy();
        code = null;
        tree?.destroy();
        tree = null;
        compare?.destroy();
        compare = null;
        kanban?.destroy();
        kanban = null;
        unmountLiquidFence(dom);
      };

      const enterRawEdit = () => {
        if (rawEdit?.active() || editor.isDestroyed || !editor.isEditable) return;
        // Measure before teardown so the editor keeps the rendered fence’s box.
        const lockSize = measureFenceHost(dom);
        const scrollParent = dom.closest(".vault-live-editor") as HTMLElement | null;
        const scrollTop = scrollParent?.scrollTop;
        const snapshot = attrs.raw;
        const restoreScroll = () => {
          if (scrollParent && typeof scrollTop === "number") {
            scrollParent.scrollTop = scrollTop;
          }
        };
        destroySurfaces();
        rawEdit = mountFenceRawEdit(dom, snapshot, {
          lockSize,
          scrollParent,
          scrollTop,
          onCommit: (nextRaw) => {
            rawEdit = null;
            if (nextRaw === attrs.raw) {
              remount(attrs);
            } else {
              applyRawUpdate(nextRaw);
            }
            requestAnimationFrame(restoreScroll);
          },
          onCancel: () => {
            rawEdit = null;
            remount(attrs);
            requestAnimationFrame(restoreScroll);
          },
        });
      };

      const remount = (nextAttrs: FenceBlockAttrs) => {
        const gen = ++mountGen;
        destroySurfaces();
        dom.replaceChildren();
        dom.setAttribute("data-lang", nextAttrs.lang || "code");
        dom.dataset.liveFenceRaw = nextAttrs.raw;

        const lang = (nextAttrs.lang || "").toLowerCase();
        const opts = this.options as FenceBlockOptions;

        if (lang === "callout") {
          const model = parseCalloutRaw(nextAttrs.raw);
          callout = mountCalloutSurface(dom, model, (updated) => {
            applyRawUpdate(serializeCalloutRaw(updated));
          });
          return;
        }

        if (lang === "chart") {
          chart = mountChartSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "card") {
          const ctx = opts.getLiquidContext?.() ?? {};
          card = mountCardSurface(
            dom,
            nextAttrs.raw,
            (updatedRaw) => applyRawUpdate(updatedRaw),
            ctx.onOpenCardDetail,
          );
          return;
        }

        if (lang === "dashboard") {
          dashboard = mountDashboardSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "tabs") {
          tabs = mountTabsSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "steps") {
          steps = mountStepsSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "accordion") {
          accordion = mountAccordionSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "code") {
          code = mountCodeSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
            enterRawEdit,
          );
          return;
        }

        if (lang === "kanban") {
          kanban = mountKanbanSurface(
            dom,
            nextAttrs.raw,
            (updatedRaw) => applyRawUpdate(updatedRaw),
            enterRawEdit,
          );
          return;
        }

        if (lang === "tree") {
          tree = mountTreeSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "compare") {
          compare = mountCompareSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "report") {
          report = mountReportSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "slides") {
          slides = mountSlidesSurface(
            dom,
            nextAttrs.raw,
            opts.getLiquidContext?.() ?? {},
            (updatedRaw) => applyRawUpdate(updatedRaw),
          );
          return;
        }

        if (lang === "medousa-view") {
          const placeholder = document.createElement("div");
          placeholder.className = "vault-live-organism vault-live-view-pending markdown-content";
          placeholder.textContent = "Loading view…";
          dom.append(placeholder);
          const ctx = opts.getResolveContext?.();
          if (!ctx) {
            placeholder.textContent = "View unavailable";
            return;
          }
          void resolveMedousaViews(nextAttrs.raw, ctx).then((html) => {
            if (gen !== mountGen) return;
            placeholder.innerHTML = html;
            placeholder.classList.remove("vault-live-view-pending");
          });
          return;
        }

        if (isLiquidFenceLang(lang)) {
          const ctx = opts.getLiquidContext?.() ?? {};
          mountLiquidFence(dom, nextAttrs.raw, ctx);
          return;
        }

        mountPlainFence(dom, lang, fenceBody(nextAttrs.raw), enterRawEdit);
      };

      remount(attrs);

      return {
        dom,
        stopEvent: liveNodeViewStopEvent,
        ignoreMutation: () => true,
        update: (updated) => {
          if (updated.type.name !== this.name) return false;
          if (rawEdit?.active()) return true;
          const next = updated.attrs as FenceBlockAttrs;
          if (next.raw === attrs.raw && next.lang === attrs.lang) {
            attrs = next;
            return true;
          }
          // Report: in-place apply avoids destroy/remount that blanks nested charts.
          if (
            attrs.lang === "report" &&
            next.lang === "report" &&
            report &&
            typeof report.applyRaw === "function"
          ) {
            attrs = next;
            report.applyRaw(next.raw);
            return true;
          }
          if (
            attrs.lang === "slides" &&
            next.lang === "slides" &&
            slides &&
            typeof slides.applyRaw === "function"
          ) {
            attrs = next;
            slides.applyRaw(next.raw);
            return true;
          }
          attrs = next;
          remount(attrs);
          return true;
        },
        destroy: () => {
          mountGen += 1;
          destroySurfaces();
        },
      };
    };
  },
});

export { attrsFromRaw as fenceAttrsFromRaw };
