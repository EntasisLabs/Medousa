import { invalidateMedousaViewCache } from "$lib/utils/resolveMedousaViews";
import {
  extractMedousaViewBlocks,
  replaceMedousaViewFenceAt,
  serializeMedousaViewFence,
  type MedousaViewQuery,
} from "$lib/utils/markdownView";
import {
  extractChartFences,
  parseChartFenceParts,
  replaceChartFenceAt,
  type ChartFenceKv,
} from "$lib/utils/vaultChartFence";
import {
  extractLiquidFences,
  parseLiquidFenceDraft,
  replaceLiquidFenceRawAt,
  serializeLiquidFenceDraft,
  type LiquidFenceDraft,
  type LiquidFenceLang,
} from "$lib/utils/vaultLiquidFence";
import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";
import { insertTextAtCursor } from "$lib/utils/vaultMarkdownEdit";
import {
  embedPathForNote,
  formatImageEmbedMarkdown,
} from "$lib/utils/vaultLocalImages";

export type VaultBridgeHost = {
  content: string;
  selectedPath: string | null;
  viewBridgeOpen: boolean;
  viewBridgeMode: "insert" | "edit";
  viewBridgeInsertAt: number;
  viewBridgeEditIndex: number | null;
  viewBridgeQuery: MedousaViewQuery | null;
  chartBridgeOpen: boolean;
  chartBridgeEditIndex: number | null;
  chartBridgeKv: ChartFenceKv | null;
  chartBridgeTableMarkdown: string;
  liquidBridgeOpen: boolean;
  liquidBridgeLang: LiquidFenceLang | null;
  liquidBridgeEditIndex: number | null;
  liquidBridgeDraft: LiquidFenceDraft | null;
  cardDetailOpen: boolean;
  cardDetail: CardDetailPayload | null;
  markDirty(
    nextContent: string,
    options?: { reloadEditors?: boolean; allowEmpty?: boolean; path?: string | null },
  ): void;
  enterEditMode(): void;
  queueEditorInsert(text: string): void;
};

export class VaultBridgeController {
  #host: VaultBridgeHost;

  constructor(host: VaultBridgeHost) {
    this.#host = host;
  }

  openViewBridgeInsert(insertAt: number) {
    this.#host.viewBridgeMode = "insert";
    this.#host.viewBridgeInsertAt = insertAt;
    this.#host.viewBridgeEditIndex = null;
    this.#host.viewBridgeQuery = null;
    this.#host.viewBridgeOpen = true;
  }

  openViewBridgeEdit(index: number) {
    const blocks = extractMedousaViewBlocks(this.#host.content);
    const block = blocks[index];
    if (!block) return;
    this.#host.viewBridgeMode = "edit";
    this.#host.viewBridgeEditIndex = index;
    this.#host.viewBridgeQuery = block.query;
    this.#host.viewBridgeOpen = true;
  }

  closeViewBridge() {
    this.#host.viewBridgeOpen = false;
    this.#host.viewBridgeQuery = null;
    this.#host.viewBridgeEditIndex = null;
  }

  commitViewBridge(query: MedousaViewQuery) {
    const host = this.#host;
    if (host.viewBridgeMode === "edit" && host.viewBridgeEditIndex != null) {
      const next = replaceMedousaViewFenceAt(
        host.content,
        host.viewBridgeEditIndex,
        query,
      );
      if (next) {
        host.markDirty(next, { reloadEditors: true });
        invalidateMedousaViewCache();
      }
    } else {
      const fence = serializeMedousaViewFence(query);
      const result = insertTextAtCursor(
        host.content,
        host.viewBridgeInsertAt,
        fence,
      );
      host.markDirty(result.content, { reloadEditors: true });
    }
    this.closeViewBridge();
  }

  openChartBridgeEdit(index: number) {
    const blocks = extractChartFences(this.#host.content);
    const block = blocks[index];
    if (!block) return;
    const parts = parseChartFenceParts(block.body);
    this.#host.chartBridgeEditIndex = index;
    this.#host.chartBridgeKv = parts.kv;
    this.#host.chartBridgeTableMarkdown = parts.tableMarkdown;
    this.#host.chartBridgeOpen = true;
  }

  closeChartBridge() {
    this.#host.chartBridgeOpen = false;
    this.#host.chartBridgeKv = null;
    this.#host.chartBridgeTableMarkdown = "";
    this.#host.chartBridgeEditIndex = null;
  }

  commitChartBridge(kv: ChartFenceKv, tableMarkdown?: string) {
    if (this.#host.chartBridgeEditIndex == null) {
      this.closeChartBridge();
      return;
    }
    const next = replaceChartFenceAt(
      this.#host.content,
      this.#host.chartBridgeEditIndex,
      kv,
      tableMarkdown,
    );
    if (next) this.#host.markDirty(next, { reloadEditors: true });
    this.closeChartBridge();
  }

  openLiquidBridgeEdit(lang: LiquidFenceLang, index: number) {
    const blocks = extractLiquidFences(this.#host.content, lang);
    const block = blocks[index];
    if (!block) return;
    this.#host.liquidBridgeLang = lang;
    this.#host.liquidBridgeEditIndex = index;
    this.#host.liquidBridgeDraft = parseLiquidFenceDraft(lang, block.body);
    this.#host.liquidBridgeOpen = true;
  }

  closeLiquidBridge() {
    this.#host.liquidBridgeOpen = false;
    this.#host.liquidBridgeLang = null;
    this.#host.liquidBridgeEditIndex = null;
    this.#host.liquidBridgeDraft = null;
  }

  commitLiquidBridge(next: LiquidFenceDraft) {
    if (this.#host.liquidBridgeEditIndex == null || !this.#host.liquidBridgeLang) {
      this.closeLiquidBridge();
      return;
    }
    const raw = serializeLiquidFenceDraft(next);
    const replaced = replaceLiquidFenceRawAt(
      this.#host.content,
      this.#host.liquidBridgeLang,
      this.#host.liquidBridgeEditIndex,
      raw,
    );
    if (replaced) this.#host.markDirty(replaced, { reloadEditors: true });
    this.closeLiquidBridge();
  }

  openCardDetail(detail: CardDetailPayload) {
    this.#host.cardDetail = detail;
    this.#host.cardDetailOpen = true;
  }

  closeCardDetail() {
    this.#host.cardDetailOpen = false;
    this.#host.cardDetail = null;
  }

  async insertImageEmbed(imagePath: string) {
    if (!this.#host.selectedPath || !imagePath.trim()) return;
    this.#host.enterEditMode();
    const embedPath = await embedPathForNote(imagePath, this.#host.selectedPath);
    const alt = embedPath.split("/").pop()?.replace(/\.[^.]+$/, "") ?? "image";
    this.#host.queueEditorInsert(formatImageEmbedMarkdown(embedPath, alt));
  }
}
