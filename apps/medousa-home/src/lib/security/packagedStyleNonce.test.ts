/** @vitest-environment happy-dom */

import { afterEach, describe, expect, it } from "vitest";
import {
  readPackagedStyleNonce,
  withPackagedStyleNonce,
} from "./packagedStyleNonce";

describe("packaged runtime style nonce", () => {
  afterEach(() => {
    document.head.querySelectorAll("style[data-csp-test]").forEach((node) => node.remove());
  });

  it("stamps styles created during a scoped library call", () => {
    const packagedStyle = document.createElement("style");
    packagedStyle.dataset.cspTest = "true";
    packagedStyle.setAttribute("nonce", "release-style-nonce");
    document.head.prepend(packagedStyle);

    const runtimeStyle = withPackagedStyleNonce(() => {
      const style = document.createElement("style");
      style.dataset.cspTest = "true";
      document.head.append(style);
      return style;
    });

    expect(readPackagedStyleNonce()).toBe("release-style-nonce");
    expect(runtimeStyle?.nonce).toBe("release-style-nonce");
  });

  it("restores normal element creation after the scoped call", () => {
    const packagedStyle = document.createElement("style");
    packagedStyle.dataset.cspTest = "true";
    packagedStyle.setAttribute("nonce", "release-style-nonce");
    document.head.prepend(packagedStyle);

    withPackagedStyleNonce(() => document.createElement("style"));
    const laterStyle = document.createElement("style");
    laterStyle.dataset.cspTest = "true";
    document.head.append(laterStyle);

    expect(laterStyle.nonce || laterStyle.getAttribute("nonce") || "").toBe("");
  });
});
