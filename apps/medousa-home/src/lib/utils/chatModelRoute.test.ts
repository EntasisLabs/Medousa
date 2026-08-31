import { describe, expect, it } from "vitest";
import {
  agentModelConfigOption,
  agentModelDisplayLabel,
  chatModelRouteKey,
  credentialRouteFor,
  modelSourceDetail,
} from "./chatModelRoute";

const options = [
  {
    id: "reasoning_effort",
    name: "Reasoning",
    type: "select",
    category: "thought_level",
    currentValue: "high",
    options: [{ value: "high", name: "High" }],
  },
  {
    id: "model",
    name: "Model",
    type: "select",
    category: "model",
    currentValue: "gpt-5.6-sol",
    options: [{ value: "gpt-5.6-sol", name: "GPT-5.6 Sol" }],
  },
];

describe("chat model route", () => {
  it("finds and labels the ACP-advertised model", () => {
    expect(agentModelConfigOption(options)?.id).toBe("model");
    expect(agentModelDisplayLabel("codex", options)).toBe("GPT-5.6 Sol");
  });

  it("keeps account and API-key routes explicit", () => {
    expect(modelSourceDetail("codex", "OpenAI", "openai")).toBe(
      "OpenAI account · Codex runtime",
    );
    expect(modelSourceDetail("hermes", "OpenRouter", "openrouter")).toBe(
      "Hermes providers · Hermes runtime",
    );
    expect(modelSourceDetail("medousa", "OpenAI", "openai")).toBe(
      "OpenAI · API key",
    );
    expect(modelSourceDetail("medousa", "Ollama", "ollama")).toBe("Ollama · Local");
    expect(credentialRouteFor("codex", "openai")).toBe("chatgpt-account");
    expect(credentialRouteFor("hermes", "openrouter")).toBe("hermes-account");
    expect(credentialRouteFor("medousa", "openai")).toBe("api-key");
    expect(credentialRouteFor("medousa", "openai-codex")).toBe("chatgpt-account");
    expect(
      chatModelRouteKey({
        runtime: "codex",
        provider: "openai",
        credential: "chatgpt-account",
        model: "gpt-5.6-sol",
      }),
    ).toBe("codex/openai/chatgpt-account/gpt-5.6-sol");
  });
});
