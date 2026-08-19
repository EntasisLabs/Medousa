import { describe, expect, it } from "vitest";
import {
  alignStageRoutingWithHost,
  defaultStageRouting,
  uniformStageTarget,
} from "./stageRouting";

describe("stage routing host alignment", () => {
  it("rebases a leftover uniform DeepSeek matrix onto GPT Luna", () => {
    const aligned = alignStageRoutingWithHost(
      defaultStageRouting("deepseek", "deepseek-v4-flash"),
      "openai",
      "gpt-5.6-luna",
    );
    expect(aligned.final_response).toMatchObject({
      provider: "openai",
      model: "gpt-5.6-luna",
    });
    expect(aligned.extractor).toMatchObject({
      provider: "openai",
      model: "gpt-5.6-luna",
    });
    expect(uniformStageTarget(aligned)).toEqual({
      provider: "openai",
      model: "gpt-5.6-luna",
    });
  });

  it("keeps a mixed worker role and pins Chat to the picker", () => {
    const mixed = defaultStageRouting("openai", "gpt-5.6-luna");
    mixed.extractor = {
      ...mixed.extractor,
      provider: "deepseek",
      model: "deepseek-v4-flash",
    };
    mixed.final_response = {
      ...mixed.final_response,
      provider: "deepseek",
      model: "deepseek-v4-flash",
    };
    const aligned = alignStageRoutingWithHost(mixed, "openai", "gpt-5.6-luna");
    expect(aligned.extractor).toMatchObject({
      provider: "deepseek",
      model: "deepseek-v4-flash",
    });
    expect(aligned.final_response).toMatchObject({
      provider: "openai",
      model: "gpt-5.6-luna",
    });
  });
});
