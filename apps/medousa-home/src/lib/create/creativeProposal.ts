import type { CreativeJob, CreativeNode, CreativeOperation, CreativeScene } from "$lib/types/generated/daemon_api";
import { descendants } from "./creativeSelection";
import { applySceneOperations } from "./creativeScene";
type Job = Pick<CreativeJob, "base_nodes" | "result" | "request">;
const conflict = "Your edits and this result both changed the same part of an object. Your generation is still saved.";
const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
function equal(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((value, i) => equal(value, b[i]));
  return object(a) && object(b) && Object.keys(a).length === Object.keys(b).length && Object.keys(a).every(key => key in b && equal(a[key], b[key]));
}
function merge(base: unknown, current: unknown, proposed: unknown): unknown {
  if (equal(proposed, base)) return current;
  if (equal(current, base) || equal(current, proposed)) return proposed;
  if (object(base) && object(current) && object(proposed)) {
    const result: Record<string, unknown> = {};
    for (const key of new Set([...Object.keys(base), ...Object.keys(current), ...Object.keys(proposed)])) {
      const value = merge(base[key], current[key], proposed[key]);
      if (value !== undefined) result[key] = value;
    }
    return result;
  }
  throw new Error(conflict);
}
/** Preview only. The daemon repeats this check atomically when the user applies. */
export function reconcileCreativeProposal(scene: CreativeScene, revision: number, job: Job): { operations: CreativeOperation[]; error: string | null } {
  if (!job.result) return { operations: [], error: null };
  if (job.result.expected_revision === revision) return { operations: job.result.operations, error: null };
  if (!job.base_nodes) return { operations: [], error: "Reload this result with the updated engine to preview it on your current canvas." };
  try {
    const operations: CreativeOperation[] = [];
    for (const op of job.result.operations) {
      if (op.op === "put_node") {
        const base = job.base_nodes.find(n => n.id === op.node.id), current = scene.nodes.find(n => n.id === op.node.id);
        const node = merge(base, current, op.node) as CreativeNode;
        if (!equal(node, current)) operations.push({ op: "put_node", node });
      } else if (op.op === "remove_node") {
        if (!scene.nodes.some(n => n.id === op.id)) continue;
        const subtree = descendants([op.id], scene.nodes);
        if (scene.nodes.some(n => subtree.has(n.id) && !equal(n, job.base_nodes!.find(b => b.id === n.id)))) throw new Error(conflict);
        operations.push(op);
      } else throw new Error(conflict);
    }
    const next = applySceneOperations(scene, operations);
    for (const op of operations) {
      const id = op.op === "put_node" ? op.node.id : op.op === "remove_node" ? op.id : "";
      let current = scene.nodes.find(n => n.id === id);
      if (op.op === "put_node" && op.node.parent_id) {
        if (!next.nodes.some(n => n.id === op.node.parent_id)) throw new Error("The original frame was deleted. Your generation is still saved.");
        current ??= scene.nodes.find(n => n.id === op.node.parent_id);
      }
      while (current) {
        if (current.locked) throw new Error("Unlock the affected objects before applying. Your generation is still saved.");
        current = scene.nodes.find(n => n.id === current!.parent_id);
      }
    }
    return { operations, error: null };
  } catch (error) { return { operations: [], error: (error as Error).message }; }
}
