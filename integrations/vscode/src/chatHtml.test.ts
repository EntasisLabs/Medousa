import { describe, expect, it } from "vitest";
import { chatHtml } from "./chatHtml.js";

describe("VS Code chat webview", () => {
  const html = chatHtml("test-nonce", {
    liquidScriptUri: "vscode-webview://test/dist/liquidWebview.js",
    cspSource: "vscode-webview://test",
  });

  it("uses a nonce-bound script policy", () => {
    expect(html).toContain("script-src 'nonce-test-nonce'");
    expect(html).toContain('<script nonce="test-nonce">');
    expect(html).toContain('src="vscode-webview://test/dist/liquidWebview.js"');
    expect(html).toContain("img-src vscode-webview://test https: data: blob:");
    expect(html).not.toContain("script-src 'unsafe-inline'");
  });

  it("emits syntactically valid webview JavaScript", () => {
    const script = html.match(/<script nonce="test-nonce">([\s\S]*?)<\/script>/)?.[1];
    expect(script).toBeTruthy();
    expect(() => new Function(script ?? "")).not.toThrow();
  });

  it("contains the persistent shell and context-aware composer", () => {
    expect(html).toContain('id="connection-label"');
    expect(html).toContain('id="messages"');
    expect(html).toContain('id="context"');
    expect(html).toContain('id="prompt"');
    expect(html).toContain('id="send"');
    expect(html).toContain(".empty[hidden] { display: none; }");
    expect(html).toContain('id="mode-button"');
    expect(html).toContain('id="work-button"');
  });

  it("supports shared runtime state and mode proposals", () => {
    expect(html).toContain('message.type === "runtimeState"');
    expect(html).toContain('message.type === "modeProposal"');
    expect(html).toContain('type: "selectUndertaking"');
    expect(html).toContain('["Switch", "Not now"]');
    expect(html).toContain("function clearModeProposal()");
    expect(html).toContain("setInterval(updateCopy, 1_000)");
  });

  it("contains conversation history, naming, and reply actions", () => {
    expect(html).toContain('id="sessions-backdrop"');
    expect(html).toContain('id="session-search"');
    expect(html).toContain('data-session-action="rename"');
    expect(html).toContain('data-session-action="delete"');
    expect(html).toContain('data-turn-action="copy"');
    expect(html).toContain('data-turn-action="share"');
    expect(html).toContain('data-turn-action="library"');
  });

  it("preserves intent through navigation and only settles completed replies", () => {
    expect(html).toContain("drafts: drafts");
    expect(html).toContain("function saveDraft()");
    expect(html).toContain("function restoreDraft()");
    expect(html).toContain("scrollPositions: scrollPositions");
    expect(html).toContain("function saveConversationState()");
    expect(html).toContain('id="scroll-latest"');
    expect(html).toContain('classList.remove("streaming")');
    expect(html).toContain('textContent = "Opening…"');
  });

  it("escapes authored content before markdown rendering", () => {
    expect(html).toContain("escapeHtml(value)");
    expect(html).toContain("safeUrl(href)");
    expect(html).toContain('["http:","https:","medousa:"]');
  });

  it("hydrates shared Liquid Markdown after normal and streaming renders", () => {
    expect(html).toContain("window.medousaLiquidMarkdown");
    expect(html).toContain("function hydrateAssistantMarkdown(bubble)");
    expect(html).toContain("liquid.hydrate(bubble");
    expect(html).toContain("hydrateAssistantMarkdown(assistant.bubble)");
    expect(html).toContain('type: "openLink", href: url');
  });
});
