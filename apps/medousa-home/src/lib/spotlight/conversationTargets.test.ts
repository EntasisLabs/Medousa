import { expect, it } from "vitest";
import type { BotProfile } from "$lib/types/generated/daemon_api";
import type { SessionSummary } from "$lib/types/session";
import { conversationTargets, mentionQuery } from "./conversationTargets";
const session = (id: string): SessionSummary => ({ session_id: id, display_name: "Labbing", turns: 2, verification_runs: 0, preview: "Work in progress" });
it("keeps identical names distinct, hides bot duplicates, and offers available new runtimes", () => {
  const targets = conversationTargets("a", [{ bot_id: "taco", display_name: "Taco", primary_session_id: "bot-thread" } as BotProfile], [session("one"), session("two"), session("bot-thread")], [{ runtime: "codex", available: true }, { runtime: "cursor", available: false }], "");
  expect(targets.map(target => target.id)).toEqual(["bot:taco", "session:one", "session:two", "new:medousa", "new:codex"]);
  expect(targets.every(target => target.scope === "a")).toBe(true);
});
it("keeps new destinations reachable in a large session list and preserves message text after a mention", () => {
  const targets = conversationTargets("a", [], Array.from({ length: 300 }, (_, id) => session(String(id))), [], "");
  expect(targets.filter(target => target.kind === "session")).toHaveLength(12);
  expect(targets.at(-1)?.id).toBe("new:medousa");
  expect(mentionQuery("@taco can you handle this?")).toEqual({ name: "taco", message: "can you handle this?" });
  expect(mentionQuery("email@example.com")).toBeNull();
});

it("surfaces the exact new Medousa address ahead of similarly named existing conversations", () => {
  const sessions = Array.from({ length: 30 }, (_, id) => ({ ...session(String(id)), display_name: "Medousa Coder" }));
  expect(conversationTargets("a", [], sessions, [], "medousa")[0]?.id).toBe("new:medousa");
});
