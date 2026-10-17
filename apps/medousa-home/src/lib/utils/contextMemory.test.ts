import { expect, it } from "vitest";
import { memoryView, memorySourceSession, memoryRelationTarget, memoryEvidenceLabel } from "./contextMemory";
import type { LocusNodeDetailResponse } from "$lib/types/locus";
const node = { sync_key: "one", session_id: "coder:synthetic", tier: "raw", timestamp: "2026-10-17T00:00:00Z", context_summary: "verification: Verification completed: Check baseline", rho: 0, kappa: 0, psi: 0 };
const fields = [
  { name: "memory_kind", value: "verification" }, { name: "summary", value: "Verification completed: Check baseline" },
  { name: "details", value: "tool=cognition_coder_shell_run; outcome=completed" },
  { name: "repo_id", value: "/Users/example/Medousa/.git" }, { name: "branch", value: "feature/context" },
  { name: "author_session", value: "real-chat" }, { name: "evidence_refs", value: ["engineering:call:123"] },
  { name: "new_observation", value: { fact: "Retain future fields" } },
  { name: "vibe_signature", value: "Injected posture metadata" },
];
function savedRecord(fields: { name: string; value: unknown }[]) {
  return `⊕⟨ ⏣0{ compression_depth: 1, semantic_links: [{ rel: "derived_from", target: "other" }] } ⟩\n◈⟨ ⏣0{ ${fields.map((field) => `${field.name}(.99): ${JSON.stringify(field.value)}`).join(", ")} } ⟩`;
}
const detail: LocusNodeDetailResponse = { node, raw: savedRecord(fields) };

it("separates recorded execution from verification verdicts, technical fields, and readable content", () => {
  const view = memoryView(detail);
  expect(view).toMatchObject({ title: "Check baseline", kind: "Verification", project: "Medousa", branch: "feature/context", execution: { outcome: "completed", tool: "cognition_coder_shell_run" } });
  expect(view.sections).toEqual([{ name: "new_observation", value: { fact: "Retain future fields" }, confidence: 0.99, label: "New observation" }]);
  expect(view.evidence).toEqual(["engineering:call:123"]);
  expect(view.technicalFields.some((field) => field.name === "author_session")).toBe(true);
  expect(view.technicalFields.some((field) => field.name === "vibe_signature")).toBe(true);
  expect(detail.raw).toBe(savedRecord(fields));
});
it("retains unrecognized execution details and retains the original record when its content cannot be interpreted", () => {
  expect(memoryView({ ...detail, raw: "unrecognized original record" })).toMatchObject({ structured: false, title: "Check baseline", sections: [], execution: null });
  const view = memoryView({ ...detail, raw: savedRecord([{ name: "details", value: "The result needs a human interpretation; outcome=completed" }]) });
  expect(view.execution).toBeNull(); expect(view.sections[0].value).toContain("human interpretation");
  const unfamiliar = memoryView({ ...detail, raw: savedRecord([{ name: "constructor", value: "An unfamiliar content field" }]) });
  expect(unfamiliar.sections[0].label).toBe("Constructor");
});
it("uses exact recorded author sessions and relationship IDs, never fuzzy prefixes", () => {
  expect(memorySourceSession(detail, new Set(["real-chat"]))).toBe("real-chat");
  expect(memorySourceSession(detail, new Set(["real-cha"]))).toBeNull();
  expect(memorySourceSession(detail, new Set([node.session_id]))).toBe(node.session_id);
  expect(memoryRelationTarget("memory:other", new Set(["other"]))).toBe("other");
  expect(memoryRelationTarget("othe", new Set(["other"]))).toBeNull();
  expect(memoryEvidenceLabel("engineering:call:123")).toBe("Tool execution reference");
});
it("uses decision-specific sections while retaining nested values", () => {
  const view = memoryView({ ...detail, raw: savedRecord([{ name: "memory_kind", value: "decision" }, { name: "decision", value: { parser_mode: "strict_and_tolerant" } }, { name: "rationale", value: "Preserve existing memory" }]) });
  expect(view.kind).toBe("Decision"); expect(view.sections.map((field) => field.label)).toEqual(["Decision", "Why"]);
});
