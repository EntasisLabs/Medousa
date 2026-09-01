/** @vitest-environment happy-dom */

import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it } from "vitest";
import {
  codeMirrorCspExtension,
  readPackagedStyleNonce,
} from "./codeMirrorCsp";

describe("CodeMirror packaged CSP", () => {
  afterEach(() => {
    document.head.querySelectorAll("style[data-csp-test]").forEach((node) => node.remove());
  });

  it("reuses Tauri's packaged style nonce for CodeMirror style-mod", () => {
    const style = document.createElement("style");
    style.dataset.cspTest = "true";
    style.setAttribute("nonce", "release-style-nonce");
    document.head.prepend(style);

    expect(readPackagedStyleNonce()).toBe("release-style-nonce");

    const state = EditorState.create({
      extensions: [codeMirrorCspExtension()],
    });
    expect(state.facet(EditorView.cspNonce)).toBe("release-style-nonce");
  });

  it("leaves browser development unrestricted when no nonce exists", () => {
    expect(readPackagedStyleNonce()).toBe("");

    const state = EditorState.create({
      extensions: [codeMirrorCspExtension()],
    });
    expect(state.facet(EditorView.cspNonce)).toBe("");
  });
});
