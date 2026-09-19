// @vitest-environment happy-dom

import { beforeEach, describe, expect, it } from "vitest";
import {
  nativeLivePreviewEnabled,
  setNativeLivePreviewEnabled,
} from "./liveVoicePreferences";

describe("native Live preview preference", () => {
  beforeEach(() => localStorage.clear());

  it("defaults off and persists explicit activation", () => {
    expect(nativeLivePreviewEnabled()).toBe(false);
    setNativeLivePreviewEnabled(true);
    expect(nativeLivePreviewEnabled()).toBe(true);
    setNativeLivePreviewEnabled(false);
    expect(nativeLivePreviewEnabled()).toBe(false);
  });
});
