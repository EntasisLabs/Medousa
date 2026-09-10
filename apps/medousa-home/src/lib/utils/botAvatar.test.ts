import { describe, expect, it } from "vitest";
import { BOT_AVATARS, botAvatar, DEFAULT_BOT_AVATAR } from "./botAvatar";

describe("Bot avatars", () => {
  it("resolves saved marks and keeps legacy emoji avatars", () => {
    for (const avatar of BOT_AVATARS) expect(botAvatar(avatar.id)).toMatchObject(avatar);
    expect(botAvatar("🧭").legacy).toBe("🧭");
    expect(botAvatar("🛠️").legacy).toBe("🛠️");
  });
  it("uses a local default for empty, unknown, or URL references", () => {
    for (const ref of [null, "", "medousa:future", "https://example.com/avatar.svg", "../../image.svg"]) {
      expect(botAvatar(ref)).toMatchObject({ id: DEFAULT_BOT_AVATAR, legacy: null });
    }
  });
});
