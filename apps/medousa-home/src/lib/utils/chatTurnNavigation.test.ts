import { describe, expect, it } from "vitest";
import { resolveChatTurnNavigation } from "$lib/utils/chatTurnNavigation";

describe("resolveChatTurnNavigation", () => {
  it("pins the user prompt that owns the response currently in view", () => {
    const result = resolveChatTurnNavigation(
      [
        { id: "older-user", top: -620, bottom: 340, height: 960 },
        { id: "newer-user", top: 380, bottom: 1_120, height: 740 },
      ],
      100,
      600,
    );

    expect(result).toEqual({
      activeId: "older-user",
      pinnedId: "older-user",
    });
  });

  it("advances the sticky prompt as the next response becomes contextual", () => {
    const result = resolveChatTurnNavigation(
      [
        { id: "older-user", top: -1_020, bottom: 70, height: 1_090 },
        { id: "newer-user", top: 20, bottom: 820, height: 800 },
      ],
      100,
      600,
    );

    expect(result).toEqual({
      activeId: "newer-user",
      pinnedId: "newer-user",
    });
  });

  it("keeps the turn active without pinning before its prompt leaves the top", () => {
    const result = resolveChatTurnNavigation(
      [{ id: "current-user", top: 112, bottom: 900, height: 788 }],
      100,
      600,
    );

    expect(result).toEqual({
      activeId: "current-user",
      pinnedId: null,
    });
  });
});
