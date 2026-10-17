import type { LocusNodeDetailResponse } from "$lib/types/locus";
import { contextMemoryPresentation } from "./contextBrowse";
import { decodeMemory, type MemoryDocument, type MemoryField } from "./sttpMemory";

const documents = new WeakMap<LocusNodeDetailResponse, MemoryDocument | null>();
function memoryDocument(detail: LocusNodeDetailResponse) {
  if (!documents.has(detail)) documents.set(detail, decodeMemory(detail.raw));
  return documents.get(detail) ?? null;
}

export function memoryLabel(name: string): string {
  const text = name.replace(/_/g, " ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

const technical = new Set(["memory_kind", "summary", "repo_id", "work_id", "branch", "observed_head", "dedupe_key", "environment_id", "environment_generation", "author_agent", "author_session", "author_turn", "author_attempt", "evidence_refs", "paths", "symbols", "vibe_signature", "user_avec", "model_avec", "compression_avec", "attractor_config", "rho", "kappa", "psi"]);
const kinds: Record<string, string> = { decision: "Decision", verification: "Verification", handoff: "Handoff", checkpoint: "Checkpoint", change: "Change", open_gap: "Open gap", hypothesis: "Hypothesis", summary: "Summary" };

export function memoryView(detail: LocusNodeDetailResponse) {
  const document = memoryDocument(detail);
  const fields = document?.fields ?? [];
  const field = (name: string) => fields.find((entry) => entry.name === name)?.value;
  const text = (name: string): string => typeof field(name) === "string" ? field(name) as string : "";
  const list = (name: string): string[] => Array.isArray(field(name)) ? (field(name) as unknown[]).filter((entry): entry is string => typeof entry === "string") : text(name) ? [text(name)] : [];
  const summary = text("summary") || detail.node.context_summary;
  const presentation = contextMemoryPresentation({ ...detail.node, context_summary: summary });
  const kindId = text("memory_kind") || detail.node.semantic_tags?.find((tag) => tag.startsWith("kind:"))?.slice(5) || "";
  const kind = presentation.kind === "Verification failed" ? presentation.kind : Object.hasOwn(kinds, kindId) ? kinds[kindId] : presentation.kind;
  // This is the exact automatic checkpoint format emitted by coder_tools. Other
  // details remain readable content; we never infer a test verdict from the kind.
  const execution = text("details").match(/^tool=([^;\n]+); outcome=(completed|failed)(?:; error=([\s\S]+))?$/);
  const content = fields.filter((entry) => !technical.has(entry.name) && !(entry.name === "details" && execution));
  const headings: Record<string, string> = {
    decision: "Decision", rationale: "Why", reason: "Why", alternatives: "Alternatives considered", tradeoffs: "Tradeoffs",
    observations: "Observations", findings: "Findings", facts: "Saved facts", result: "Recorded result",
    completed: "Completed work", completed_work: "Completed work", remaining: "Remaining work", remaining_work: "Remaining work", next_steps: "Next steps", blockers: "Blockers", blocker: "Blocker",
    details: kindId === "decision" ? "Decision context" : kindId === "handoff" ? "Handoff details" : kindId === "verification" ? "Verification details" : "Details",
  };
  const sections = content.map((entry) => ({ ...entry, label: Object.hasOwn(headings, entry.name) ? headings[entry.name] : memoryLabel(entry.name) }));
  const repo = text("repo_id").replace(/\\/g, "/").replace(/\/$/, "").replace(/\/\.git$/, "");
  return {
    ...presentation, kind, sections, fields,
    structured: Boolean(document), compressionDepth: document?.compressionDepth ?? null, parentNodeId: document?.parentNodeId ?? null,
    summary: presentation.content,
    project: repo.split("/").pop() || "", branch: text("branch"), head: text("observed_head"),
    authorSession: text("author_session"), evidence: list("evidence_refs"), paths: list("paths"), symbols: list("symbols"),
    execution: execution ? { tool: execution[1], outcome: execution[2], error: execution[3] || "" } : null,
    relations: document?.relations ?? [],
    technicalFields: fields.filter((entry) => technical.has(entry.name) && !["summary", "memory_kind"].includes(entry.name)),
  };
}

export function memorySourceSession(detail: LocusNodeDetailResponse, sessions: Set<string>): string | null {
  const author = memoryDocument(detail)?.fields.find((field) => field.name === "author_session")?.value;
  if (typeof author === "string" && sessions.has(author)) return author;
  return sessions.has(detail.node.session_id) ? detail.node.session_id : null;
}

export function memoryRelationTarget(target: string, keys: Set<string>): string | null {
  // Only resolve explicit identifiers, never a partial digest or fuzzy text match.
  if (keys.has(target)) return target;
  for (const prefix of ["thread:", "memory:", "node:"]) {
    if (target.startsWith(prefix) && keys.has(target.slice(prefix.length))) return target.slice(prefix.length);
  }
  return null;
}

export function memoryEvidenceLabel(reference: string): string {
  if (reference.startsWith("engineering:call:")) return "Tool execution reference";
  if (reference.startsWith("forge:evidence-digest:")) return "Evidence digest";
  if (reference.startsWith("forge:evidence:")) return "Saved evidence reference";
  if (reference.startsWith("forge:decision:")) return "Review decision reference";
  return "Saved reference";
}

export function memoryFieldHasValue(field: MemoryField): boolean {
  return field.value !== null && field.value !== "" && !(Array.isArray(field.value) && !field.value.length);
}
