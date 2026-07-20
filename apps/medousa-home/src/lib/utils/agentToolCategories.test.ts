import { describe, expect, it } from "vitest";
import { groupAgentTools, parseAgentToolId } from "./agentToolCategories";

describe("agentToolCategories", () => {
  it("parses dotted cognition tools into module + action", () => {
    expect(parseAgentToolId("cognition.turn.checkpoint")).toMatchObject({
      moduleId: "turn",
      moduleLabel: "Turn",
      actionId: "checkpoint",
      actionLabel: "Checkpoint",
    });
  });

  it("parses underscored cognition tools", () => {
    expect(parseAgentToolId("cognition_artifact_delete")).toMatchObject({
      moduleId: "artifact",
      actionId: "delete",
    });
  });

  it("groups and counts selected tools", () => {
    const groups = groupAgentTools(
      [
        "cognition.turn.finish",
        "cognition.turn.checkpoint",
        "cognition.artifact_delete",
      ],
      ["cognition.turn.finish"],
    );
    expect(groups.map((g) => g.moduleId).sort()).toEqual(["artifact", "turn"]);
    const turn = groups.find((g) => g.moduleId === "turn")!;
    expect(turn.selectedCount).toBe(1);
    expect(turn.tools).toHaveLength(2);
  });
});
