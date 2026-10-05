import { describe, expect, it } from "vitest";
import { existsSync } from "node:fs";
import { mascotImage } from "$lib/theme/medousaMarks";
import manifest from "../../../static/brand/mascots/manifest.json";
import { BOT_AVATARS, BOT_MASCOT_BODIES, BOT_MASCOT_EXPRESSIONS, botAvatar, DEFAULT_BOT_AVATAR } from "./botAvatar";

describe("Bot avatars", () => {
  it("resolves saved marks and keeps legacy emoji avatars", () => {
    for (const avatar of BOT_AVATARS) expect(botAvatar(avatar.id)).toMatchObject(avatar);
    expect(botAvatar("mascot:seahorse").mascot).toBe("seahorse");
    expect(botAvatar("🧭").legacy).toBe("🧭");
    expect(botAvatar("🛠️").legacy).toBe("🛠️");
  });
  it("offers every supplied mascot and expression with an image that exists", () => {
    expect(BOT_MASCOT_BODIES.map((body) => body.id)).toEqual(Object.keys(manifest.bodies));
    expect(BOT_MASCOT_EXPRESSIONS.map((expression) => expression.id)).toEqual(manifest.states);
    const mascotOptions = BOT_AVATARS.filter((avatar) => avatar.mascot);
    expect(mascotOptions).toHaveLength(BOT_MASCOT_BODIES.length * BOT_MASCOT_EXPRESSIONS.length);
    expect(new Set(mascotOptions.map((avatar) => avatar.id)).size).toBe(mascotOptions.length);
    for (const option of mascotOptions) {
      const image = mascotImage(option.mascot!, option.expression!);
      expect(existsSync(new URL(`../../../static${image}`, import.meta.url).pathname), option.id).toBe(true);
    }
    expect(botAvatar("mascot:medousa")).toMatchObject({ mascot: "medousa", expression: "default" });
    expect(botAvatar("mascot:seahorse:focus")).toMatchObject({ mascot: "seahorse", expression: "focus" });
  });
  it("uses a local default for empty, unknown, or URL references", () => {
    for (const ref of [null, "", "medousa:future", "https://example.com/avatar.svg", "../../image.svg"]) {
      expect(botAvatar(ref)).toMatchObject({ id: DEFAULT_BOT_AVATAR, legacy: null });
    }
  });
});
