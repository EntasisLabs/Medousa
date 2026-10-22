import { expect, it } from "vitest";
import { createNode, applySceneOperations } from "./creativeScene";
import { reconcileCreativeProposal } from "./creativeProposal";
import type { CreativeJob, CreativeScene } from "$lib/types/generated/daemon_api";
const node = (id: string) => ({ ...createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 1 }, { x: 0, y: 0, width: 100, height: 100 }), id });
function fixture() {
  const original = node("original"), proposed = { ...original, content: { ...original.content, fill: "#f00" } };
  const job = { base_nodes: [original], request: { id: "job", expected_revision: 1, selected_ids: [original.id], prompt: "Red" }, result: { id: "job", expected_revision: 1, selected_ids: [original.id], label: "Red", operations: [{ op: "put_node", node: proposed }] } } as Pick<CreativeJob, "base_nodes" | "request" | "result">;
  const scene: CreativeScene = { schema: "medousa-create", version: 3, background: "transparent", nodes: [original] };
  return { original, proposed, job, scene };
}
it("previews AI styling while preserving human movement and unrelated additions", () => {
  const { scene, job } = fixture();
  scene.nodes = [{ ...scene.nodes[0], transform: { ...scene.nodes[0].transform, x: 200 } }, node("human")];
  const result = reconcileCreativeProposal(scene, 3, job);
  expect(result.error).toBeNull();
  const preview = applySceneOperations(scene, result.operations);
  expect(preview.nodes[0]).toMatchObject({ transform: { x: 200 }, content: { fill: "#f00" } });
  expect(preview.nodes[1]).toBe(scene.nodes[1]);
  expect(scene.nodes[0].content).toMatchObject({ fill: "#fff" });
});
it("keeps real field conflicts, deleted targets and duplicate IDs from overwriting work", () => {
  const { scene, job } = fixture(); const current = structuredClone(scene.nodes[0]);
  if (current.content.kind === "shape") current.content.fill = "#0f0";
  scene.nodes[0] = current;
  expect(reconcileCreativeProposal(scene, 2, job).error).toContain("generation is still saved");
  expect(reconcileCreativeProposal({ ...scene, nodes: [] }, 2, job).error).toContain("generation is still saved");
  job.base_nodes = [];
  expect(reconcileCreativeProposal(scene, 2, job).error).toContain("generation is still saved");
});
it("never cascades AI deletion through newly added human children", () => {
  const { scene, job } = fixture();
  scene.nodes[0].content = { kind: "frame", name: "Frame", background: "#fff" };
  job.base_nodes = structuredClone(scene.nodes);
  const child = { ...node("child"), parent_id: scene.nodes[0].id }; scene.nodes.push(child);
  job.result!.operations = [{ op: "remove_node", id: "original" }];
  expect(reconcileCreativeProposal(scene, 2, job).error).toContain("generation is still saved");
});
it("already-applied changes become a harmless no-op and old engines cannot preview missing bases", () => {
  const { scene, proposed, job } = fixture(); scene.nodes = [proposed];
  expect(reconcileCreativeProposal(scene, 2, job)).toEqual({ operations: [], error: null });
  delete job.base_nodes;
  expect(reconcileCreativeProposal(scene, 2, job).error).toContain("updated engine");
});

it("explains locked targets and missing image parents before showing an apply preview", () => {
  const { scene, job } = fixture(); scene.nodes[0] = { ...scene.nodes[0], locked: true };
  expect(reconcileCreativeProposal(scene, 2, job).error).toContain("Unlock");
  const child = { ...node("generated"), parent_id: "removed-frame" };
  job.result!.operations = [{ op: "put_node", node: child }];
  expect(reconcileCreativeProposal(scene, 2, job).error).toContain("frame was deleted");
});
