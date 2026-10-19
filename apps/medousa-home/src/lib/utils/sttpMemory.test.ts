import { expect, it } from "vitest";
import { decodeMemory } from "./sttpMemory";

it("decodes STTP content, nested qualified keys, escaped strings, and provenance", () => {
  const raw = String.raw`⊕⟨ ⏣0{ trigger: manual, compression_depth: 2, parent_node: ref:⏣1, semantic_links: [{ rel: "derived_from", target: "earlier", confidence: 0.9 }], prime: { context_summary: "provenance \"◈⟨\" stays quoted" } } ⟩
⦿⟨ ⏣0{ session_id: "source", tier: raw } ⟩
◈⟨ ⏣0{ summary(.98): "A \"quoted\" decision: keep field(.9): text", details(.95): "line one\nline two; ◈⟨ ⟩ are text", decision(.96): { parser_mode(.95): "strict_and_tolerant" }, paths(.98): ["a,b.rs", "src/main.rs"], count(.9): 0, okay(.9): false, empty(.9): null } ⟩
⍉⟨ ⏣0{ rho: 0.98, kappa: 0.99, psi: 2.93 } ⟩`;
  const view = decodeMemory(raw)!;
  expect(view).toMatchObject({ compressionDepth: 2, parentNodeId: "ref:⏣1", relations: [{ relation: "derived_from", target: "earlier", confidence: 0.9 }] });
  expect(view.fields[0]).toEqual({ name: "summary", confidence: 0.98, value: 'A "quoted" decision: keep field(.9): text' });
  expect(view.fields[1].value).toBe("line one\nline two; ◈⟨ ⟩ are text");
  expect(view.fields[2].value).toEqual({ parser_mode: "strict_and_tolerant" });
  expect(view.fields[3].value).toEqual(["a,b.rs", "src/main.rs"]);
  expect(view.fields.slice(4).map((field) => field.value)).toEqual([0, false, null]);
});

it("preserves unknown fields and Unicode without executing content or prototype keys", () => {
  const view = decodeMemory('◈⟨ ⏣0{ unknown(.9): { "__proto__": "saved text", greeting(.9): "mañana 🌻" }, value(.9): "${doNothing()}" } ⟩')!;
  expect(Object.hasOwn(view.fields[0].value as object, "__proto__")).toBe(true);
  expect((view.fields[0].value as Record<string, unknown>).greeting).toBe("mañana 🌻");
  expect(view.fields[1].value).toBe("${doNothing()}");
});

it("falls back for malformed, duplicate, partial and oversized records", () => {
  for (const raw of ["unstructured original", '◈⟨ ⏣0{ summary(.9): "unfinished } ⟩', "◈⟨ ⏣0{ a(.9): 1, a(.8): 2 } ⟩", "◈⟨ ⏣0{ a(2): 1 } ⟩", '◈⟨ ⏣0{ a(.9): "ok" } missing-layer-end', "x".repeat(512 * 1024 + 1)]) expect(decodeMemory(raw)).toBeNull();
  expect(decodeMemory(`◈⟨ ⏣0{ a(.9): ${"[".repeat(20)}0${"]".repeat(20)} } ⟩`)).toBeNull();
});
