import type { CreativeDocument, CreativeScene } from "$lib/types/generated/daemon_api";
/** Older imports keep their saved original; presentation hides it without rewriting history. */
export function editableImportScene(scene: CreativeScene): CreativeScene {
  const original = scene.nodes.find(n => n.id === "original_frame" && n.content.kind === "frame");
  if (!original || !scene.nodes.some(n => n.design_source)) return scene;
  const removed = new Set([original.id]);
  let size = 0;
  while (size !== removed.size) { size = removed.size; for (const node of scene.nodes) if (node.parent_id && removed.has(node.parent_id)) removed.add(node.id); }
  return { ...scene, nodes: scene.nodes.filter(n => !removed.has(n.id)) };
}
export function originalReferences(document: CreativeDocument) {
  const legacy = document.scene.nodes.find(n => n.id === "original_image" && n.content.kind === "image");
  return document.scene.nodes.filter(n => n.content.kind === "frame" && n.design_source).flatMap(frame => {
    const asset = frame.design_source?.original_asset_id ?? (legacy?.content.kind === "image" ? legacy.content.asset_id : undefined);
    return asset ? [{ frame, asset }] : [];
  });
}
