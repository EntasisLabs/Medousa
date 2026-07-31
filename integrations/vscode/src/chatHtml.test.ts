import { describe, expect, it } from "vitest";
import { chatHtml } from "./chatHtml.js";

describe("VS Code chat webview", () => {
  const html = chatHtml("test-nonce");

  it("uses a nonce-bound script policy", () => {
    expect(html).toContain("script-src 'nonce-test-nonce'");
    expect(html).toContain('<script nonce="test-nonce">');
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
  });

  it("escapes authored content before markdown rendering", () => {
    expect(html).toContain("escapeHtml(value)");
    expect(html).toContain("safeUrl(href)");
    expect(html).toContain('["http:","https:","medousa:"]');
  });
});
