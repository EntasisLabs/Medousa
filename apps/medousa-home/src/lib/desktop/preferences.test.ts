// @vitest-environment happy-dom
import { beforeEach, expect, it, vi } from "vitest";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
import { loadTuiDefaultsSummary, persistTuiDefaults, persistTuiFavoriteModels, persistTuiRuntimePrefs, persistTuiVoicePrefs } from "$lib/config";

beforeEach(() => { invoke.mockReset(); window.history.replaceState({}, "", "/desktop/work"); });

it("loads the retained workshop's preferences instead of the host file", async () => {
  invoke.mockResolvedValue({ provider: "remote", activeVoiceId: "calm" });
  expect(await loadTuiDefaultsSummary()).toEqual({ provider: "remote", activeVoiceId: "calm" });
  expect(invoke).toHaveBeenCalledWith("runtime_get_tui_defaults");
});

it("updates only requested voice and favorite preferences", async () => {
  await persistTuiVoicePrefs({ activeVoiceId: "calm" });
  expect(invoke).toHaveBeenLastCalledWith("runtime_patch_tui_preferences", { patch: { active_voice_id: "calm", custom_voice_presets: null } });
  await persistTuiFavoriteModels([{ provider: "a", model: "b" }]);
  expect(invoke).toHaveBeenLastCalledWith("runtime_patch_tui_preferences", { patch: { favorite_models: [{ provider: "a", model: "b" }] } });
});

it("preserves omitted runtime settings when changing models", async () => {
  await persistTuiRuntimePrefs("a", "b", "standard");
  expect(invoke).toHaveBeenCalledWith("runtime_patch_tui_preferences", { patch: { provider: "a", model: "b", response_depth_mode: "standard" } });
  await persistTuiDefaults({ provider: "a" });
  expect(invoke).toHaveBeenLastCalledWith("runtime_put_tui_defaults", { dto: { provider: "a" } });
});

it("keeps the main shell's existing preference behavior", async () => {
  window.history.replaceState({}, "", "/");
  await loadTuiDefaultsSummary();
  expect(invoke).toHaveBeenLastCalledWith("load_tui_defaults_summary");
  await persistTuiVoicePrefs({ activeVoiceId: "calm" });
  expect(invoke).toHaveBeenLastCalledWith("persist_tui_voice_prefs", { activeVoiceId: "calm", customVoicePresets: null });
});
