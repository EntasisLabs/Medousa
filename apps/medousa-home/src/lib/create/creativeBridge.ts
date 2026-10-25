/** Explicit, workshop-bound bridges. Rendering is a snapshot; Code keeps its runtime. */
import { daemonUnary } from "$lib/daemon/contractClient";
import { operationPath } from "$lib/daemon/opPath";
import type { OperationId } from "$lib/daemon/contractClient";
import type { BrowserDesignCapture, BrowserDesignPageCapture, CreativeCodeContext, CreativeDocument, CreativeNode, CreativeScene, CreateSessionResponse, HostTurnContext } from "$lib/types/generated/daemon_api";
import type { TurnTicketResponse } from "$lib/types/session";
import type { ItemProjection, ProjectTaskRunList } from "$lib/forge";
import { creativeApi } from "./creativeApi";
import { randomUuid } from "$lib/utils/randomUuid";

export function creativeBridgeApi(scope: string) {
  const send = <T>(op: OperationId, params: Record<string, string> = {}, body?: unknown, query?: Record<string, string>) => daemonUnary<T>(op, params, body, null, query, scope);
  return {
    async projects(): Promise<ItemProjection[]> {
      const result: ItemProjection[] = []; let cursor: string | null = null; const seen = new Set<string>();
      do {
        const query: Record<string, string> = { limit: "256" }; if (cursor) query.cursor = cursor;
        const page: ItemProjection[] | { items: ItemProjection[]; next_cursor?: string | null; truncated?: boolean } = await send<ItemProjection[] | { items: ItemProjection[]; next_cursor?: string | null; truncated?: boolean }>("forge.items.get", {}, undefined, query);
        if (Array.isArray(page)) return page;
        result.push(...page.items); cursor = page.truncated ? page.next_cursor ?? null : null;
        if (cursor && seen.has(cursor)) throw new Error("Project list returned a repeated page");
        if (cursor) seen.add(cursor);
      } while (cursor);
      return result;
    },
    project: (id: string) => send<ItemProjection>("forge.items.by_work_id.get", { work_id: id }),
    runs: (id: string) => send<ProjectTaskRunList>("forge.items.by_work_id.task_runs.get", { work_id: id }),
    worlds: () => send<{ worlds: { world_id: string; title: string; url: string; run_state: string }[] }>("browser.worlds.isolated.get"),
    // Capturing a page does not request presentation or switch the user's browser.
    openPreview: (url: string) => send<{ world: { world_id: string } }>("browser.worlds.isolated.post", {}, { initial_url: url, display_name: "Create preview", profile: { kind: "ephemeral" }, headless: true, view_attached: false }),
    binding: (session: string) => send<{ work_id?: string | null }>("sessions.by_session_id.code_binding.get", { session_id: session }),
    capture: (worldId: string) => send<BrowserDesignCapture>("browser.worlds.isolated.by_world_id.design_capture.post", { world_id: worldId }),
    capturePage: (worldId: string) => send<BrowserDesignPageCapture>("browser.worlds.isolated.by_world_id.design_capture.page.post", { world_id: worldId }),
    newSession: (title: string) => send<CreateSessionResponse>("sessions.post", {}, { display_name: title, catalog: "single" }),
    bind: (session: string, workId: string) => send("sessions.by_session_id.code_binding.put", { session_id: session }, { work_id: workId }),
    context: (id: string, frame: string, revision: number, visual: string, session: string) => send<CreativeCodeContext>("create.projects.by_id.code_context.post", { id }, { frame_id: frame, expected_revision: revision, visual_asset_id: visual, session_id: session }),
    start: (session: string, workId: string, prompt: string, media: CreativeCodeContext["media_refs"], hostContext?: HostTurnContext | null) => send<TurnTicketResponse>("turns.post", {}, { session_id: session, prompt, agent_mode: "coder", code_context: { work_id: workId }, mode: "interactive", persist_user_turn: true, provider: "", model: "", media_refs: media, host_context: hostContext }),
  };
}
export function designBuildPrompt(_document: CreativeDocument, _frame: CreativeNode, instruction: string, _context: CreativeCodeContext["context"]): string {
  return instruction.trim() || "Implement this design in the linked Code project, reusing its existing components and preserving its behavior.";
}
export function designBuildContext(document: CreativeDocument, frame: CreativeNode, context: CreativeCodeContext): HostTurnContext {
  if (!context.snapshot_id) throw new Error("Update the workshop engine before handing this design to Code.");
  return { source: "create", references: [{ kind: "creative_selection", document_id: document.id, title: `${document.title} · ${frame.name || frame.id}`, selected_ids: [frame.id], snapshot_id: context.snapshot_id }], diagnostics: [], related_resources: [] };
}
const forgePreviewPrefix = operationPath("forge.preview.by_token.get", { token: "token" }).replace(/token$/, "");
export function safeDesignUrl(value: string): string {
  try { const url = new URL(value); if (!["http:", "https:"].includes(url.protocol)) return ""; url.username = ""; url.password = ""; url.search = ""; url.hash = ""; if (url.pathname.includes(forgePreviewPrefix)) url.pathname = "/project-preview"; return url.toString(); } catch { return ""; }
}
function cropCapture(capture: BrowserDesignCapture, image: HTMLImageElement, region: BrowserDesignCapture["raster_regions"][number]): string {
  const sx = image.naturalWidth / capture.screenshot.viewport.width, sy = image.naturalHeight / capture.screenshot.viewport.height;
  const canvas = document.createElement("canvas"); canvas.width = Math.max(1, Math.ceil(region.width * sx)); canvas.height = Math.max(1, Math.ceil(region.height * sy));
  const context = canvas.getContext("2d"); if (!context) throw new Error("Image import is unavailable");
  context.drawImage(image, region.x * sx, region.y * sy, region.width * sx, region.height * sy, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/png").split(",")[1];
}
/** Stable IDs let Retry continue an interrupted import without replacing a user's project. */
export async function importBrowserDesign(scope: string, id: string, result: BrowserDesignCapture | BrowserDesignPageCapture, workId: string | null, current: () => boolean) {
  if (!current()) throw new Error("The workshop changed before import");
  const slices = "slices" in result ? result.slices : [{ capture: result, clip_top: 0, height: result.screenshot.viewport.height, page_y: result.screenshot.viewport.scroll_y ?? 0 }];
  if (!slices.length || slices.length > 12) throw new Error("Invalid page capture");
  const api = creativeApi(scope), first = slices[0].capture;
  const created = await api.create({ id, title: `${first.screenshot.title || "Page"} · design`.slice(0, 100), scene: { schema: "medousa-create", version: 3, background: "transparent", nodes: [] } });
  if (created.scene.nodes.some(n => n.id === "original_frame" || n.design_source?.original_asset_id)) return created;
  const nodes: CreativeNode[] = []; let frameY = 0;
  for (const [index, slice] of slices.entries()) {
    if (!current()) throw new Error("The workshop changed during import");
    const capture = slice.capture, width = capture.screenshot.viewport.width;
    const image = new Image();
    if (slice.clip_top || slice.height !== capture.screenshot.viewport.height || capture.raster_regions.length) { image.src = `data:image/png;base64,${capture.screenshot.image_base64}`; await image.decode(); }
    const originalBytes = slice.clip_top || slice.height !== capture.screenshot.viewport.height ? cropCapture(capture, image, { asset_id: "original", x: 0, y: slice.clip_top, width, height: slice.height }) : capture.screenshot.image_base64;
    const baseline = await api.importAsset(id, originalBytes);
    const assets = new Map<string, { id: string; y: number; height: number }>();
    for (const region of capture.raster_regions) {
      if (!current()) throw new Error("The workshop changed during import");
      const top = Math.max(slice.clip_top, region.y), bottom = Math.min(slice.clip_top + slice.height, region.y + region.height);
      if (bottom <= top) continue;
      const asset = await api.importAsset(id, cropCapture(capture, image, { ...region, y: top, height: bottom - top }));
      assets.set(region.asset_id, { id: asset.id, y: top - slice.clip_top, height: bottom - top });
    }
    const frame = capture.scene.nodes.find(n => n.content.kind === "frame");
    if (!frame) throw new Error("The capture has no editable frame");
    const prefix = `page${index + 1}_`;
    for (const original of capture.scene.nodes) {
      const n = structuredClone(original); n.id = prefix + n.id; n.parent_id = n.parent_id ? prefix + n.parent_id : null;
      if (n.content.kind === "image") {
        const region = assets.get(n.content.asset_id); if (!region) continue;
        n.content.asset_id = region.id; n.transform.y = region.y; n.transform.height = region.height;
      } else if (n.id !== prefix + frame.id) n.transform.y -= slice.clip_top;
      if (n.id === prefix + frame.id) {
        n.transform = { ...n.transform, x: 0, y: frameY, width, height: slice.height };
        if (slices.length > 1) { n.name = `Page · ${index + 1} of ${slices.length}`; if (n.content.kind === "frame") n.content.name = n.name; }
        n.design_source = { work_id: workId, url: safeDesignUrl(capture.screenshot.url), viewport_width: width, viewport_height: slice.height, captured_at_ms: capture.screenshot.captured_at_ms, original_asset_id: baseline.id, scroll_y: slice.page_y, page_index: index + 1, page_count: slices.length };
      }
      nodes.push(n);
    }
    frameY += slice.height + 80;
  }
  const doc = (await api.get(id)).document;
  if (!current()) throw new Error("The workshop changed during import");
  return (await api.transact(id, { id: `import_${id}`, expected_revision: doc.revision, label: "Import editable page", operations: nodes.map(node => ({ op: "put_node", node })) })).document;
}
export const newDesignImportId = () => randomUuid();
