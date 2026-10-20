import { expect, it } from "vitest";
import { companionResults, type CompanionAction } from "./companionResults";
const actions: CompanionAction[] = [
  { id: "note", label: "New note", keywords: "vault sticky", description: "Open a sticky note", icon: "note", run: () => {} },
  { id: "web", label: "Open browser", keywords: "web url", icon: "web", run: () => {} },
];
it("offers recent destinations and a fresh request without creating a session", () => {
  const results = companionResults("", "a", [{ bot_id: "taco", display_name: "Taco" } as never], [{ session_id: "lab", display_name: "Labbing" } as never], [], actions);
  expect(results.map(result => result.label)).toEqual(["Labbing", "Taco", "New request", "New note", "Open browser"]);
  expect(results[0]).toMatchObject({ kind: "target", target: { sessionId: "lab", scope: "a" } });
});
it("searches tools and destination names while leaving free-form requests as asks", () => {
  expect(companionResults("sticky", "a", [], [], [], actions)).toMatchObject([{ kind: "action", action: { id: "note" } }]);
  expect(companionResults("look up the release notes", "a", [], [], [], actions)).toMatchObject([{ kind: "ask", label: "Ask Medousa" }]);
  expect(companionResults("new request", "a", [], [], [], actions)[0]).toMatchObject({ kind: "target", target: { kind: "new", runtime: "medousa" } });
});
it("keeps an existing session name distinct from a new installed agent", () => {
  const results = companionResults("codex", "a", [], [{ session_id: "review", display_name: "Codex review" } as never], [{ runtime: "codex", available: true } as never], actions);
  expect(results.map(result => result.id)).toEqual(["session:review", "new:codex"]);
});
