import type { DrawDocument } from "$lib/draw/drawDocument";
import type { CreativeDocumentRef, CreativeNode, CreativeScene } from "$lib/types/generated/daemon_api";

/** Pure conversion of already-normalized v1/v2 ink. Does not save or rewrite a note. */
export function creativeSceneFromDraw(document: DrawDocument): CreativeScene {
  const frameId = "legacy_frame";
  const frame: CreativeNode = {
    id: frameId, parent_id: null,
    transform: { x: 0, y: 0, width: document.width, height: document.height, rotation: 0 },
    opacity: 1, visible: true, locked: false,
    content: { kind: "frame", name: "Drawing", background: document.background },
  };
  const seen = new Set<string>([frameId]);
  const strokes = document.strokes.map((stroke, index): CreativeNode => {
    // Legacy IDs may be absent, duplicated or outside the new identifier alphabet.
    // Their stable position supplies a deterministic fallback for migration retries.
    let id = /^[a-zA-Z0-9_-]{1,128}$/.test(stroke.id) ? stroke.id : `legacy_stroke_${index}`;
    while (seen.has(id)) id = `legacy_stroke_${index}_${seen.size}`;
    seen.add(id);
    return {
      id, parent_id: frameId,
      transform: { x: 0, y: 0, width: document.width, height: document.height, rotation: 0 },
      opacity: 1, visible: true, locked: false,
      content: {
        kind: "ink", color: stroke.color, input: stroke.input, brush: { ...stroke.brush },
        points: stroke.points.map(point => ({
          x: point.x, y: point.y, pressure: point.pressure ?? null,
          elapsed_ms: point.elapsedMs ?? null, tilt_x: point.tiltX ?? null, tilt_y: point.tiltY ?? null,
        })),
      },
    };
  });
  return { schema: "medousa-create", version: 3, background: "transparent", nodes: [frame, ...strokes] };
}

/** Scope includes the workshop and vault root; note paths alone are not global IDs.
 * Persist the returned document reference in the note when adoption is accepted.
 * Future note moves then follow that reference, rather than recomputing identity.
 */
export async function creativeAdoptionId(workshopId: string, rootId: string, notePath: string, drawingAnchor: string): Promise<string> {
  if (![workshopId, rootId, notePath, drawingAnchor].every(value => value.trim())) throw new Error("Drawing adoption requires its workshop, vault root, note and anchor");
  const bytes = new TextEncoder().encode(JSON.stringify(["medousa/create/adopt/v1", workshopId, rootId, notePath, drawingAnchor]));
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return `legacy_${Array.from(digest, byte => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** An inert, workshop-relative reference; large scenes and images stay outside Markdown. */
export function serializeCreativeReference(reference: CreativeDocumentRef): string {
  const valid = /^[a-zA-Z0-9_-]{1,128}$/;
  if (!valid.test(reference.document_id) || (reference.frame_id != null && !valid.test(reference.frame_id))) throw new Error("Invalid creative document reference");
  return `\`\`\`create\n${JSON.stringify({ document_id: reference.document_id, frame_id: reference.frame_id ?? null })}\n\`\`\``;
}
export function parseCreativeReference(source: string): CreativeDocumentRef {
  if (source.length > 512) throw new Error("Creative reference is too large");
  const value: unknown = JSON.parse(source);
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid creative reference");
  const row = value as Record<string, unknown>;
  if (Object.keys(row).some(key => key !== "document_id" && key !== "frame_id") || typeof row.document_id !== "string" || (row.frame_id != null && typeof row.frame_id !== "string")) throw new Error("Invalid creative reference");
  const reference = { document_id: row.document_id, frame_id: row.frame_id as string | null | undefined ?? null };
  serializeCreativeReference(reference);
  return reference;
}
