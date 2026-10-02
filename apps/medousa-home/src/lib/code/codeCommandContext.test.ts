import { expect, it } from "vitest";
import { contextualCommands, filterCommands, suggestedCommand } from "./codeCommandContext";
import type { ProjectTask } from "./codeDocumentService";
const task = (id: string, root: string, rank = 0): ProjectTask => ({ id, root, label: id, default_rank: rank, kind: "run", argv: ["tool", id], provider: "runtime" });
it("uses the deepest discovered package with path boundaries", () => {
  const tasks = [task("root", "."), task("app", "apps/app"), task("nested", "apps/app/client"), task("other", "apps/apple")];
  expect(contextualCommands(tasks, "apps/app/client/src/main.ts").map((row) => row.id)).toEqual(["nested"]);
  expect(contextualCommands(tasks, "apps/apple/src/main.ts").map((row) => row.id)).toEqual(["other"]);
});
it("asks for a choice when recommendations are equally ranked", () => {
  expect(suggestedCommand([task("a", "app", 10), task("b", "app", 10)], "app/main.ts")).toBeNull();
  expect(suggestedCommand([task("a", "app", 10), task("b", "app", 5)], "app/main.ts")?.id).toBe("a");
});
it("filters large command catalogs without dropping inaccessible tools", () => {
  const tasks = Array.from({ length: 3000 }, (_, i) => task(`run-${i}`, `packages/${i}`));
  tasks[2345].available = false;
  expect(filterCommands(tasks, "packages/2345 run-2345")).toEqual([tasks[2345]]);
});
