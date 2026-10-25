/** @vitest-environment happy-dom */
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import type { HomeNotificationIntent } from "$lib/types/workspace";
import type { TurnCompletionOutcomeV3, TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
const api = vi.hoisted(() => ({ granted: vi.fn(), request: vi.fn(), send: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({ isPermissionGranted: api.granted, requestPermission: api.request, sendNotification: api.send }));
vi.mock("$lib/platform", () => ({ isTauriMobilePlatform: () => false }));
import { rememberSpotlightTurn, spotlightTurnOrigin, SPOTLIGHT_REPLIES_KEY } from "$lib/spotlight/completionOrigins";
const origin = { workshopId: "workshop-a", sessionId: "chat-a", turnId: "turn-a", acceptedAt: "2026-10-25T02:00:00Z", label: "Medousa" };
const envelope = (outcome: TurnCompletionOutcomeV3 = "completed", emittedAt = "2026-10-25T02:01:00Z"): TurnStreamEnvelopeV3 => ({
  schema_version: 3, turn_id: "turn-a", seq: 20, emitted_at_utc: emittedAt,
  event: { type: "turn_completed", outcome, aggregate_text: "The editable design is ready." },
});
const intent: HomeNotificationIntent = { notification_id: "turn-update:turn-a", kind: "turn_update", subject_id: "turn-a", title: "Medousa — turn ready", body: "The editable design is ready.", emitted_at_utc: "2026-10-25T02:01:00Z" };
beforeEach(() => {
  vi.resetModules(); vi.clearAllMocks(); vi.useFakeTimers(); localStorage.clear();
  api.granted.mockResolvedValue(true); api.request.mockResolvedValue("granted");
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
});
afterEach(() => { vi.useRealTimers(); });
it("notifies a global Spotlight request while hidden even when ordinary turn updates are off", async () => {
  const notifications = await import("./notifications");
  localStorage.setItem("medousa-home-notify-turn-updates", "0");
  await notifications.presentHomeNotification(intent, origin.workshopId);
  await vi.runAllTimersAsync(); expect(api.send).not.toHaveBeenCalled();
  notifications.registerSpotlightTurn(origin);
  expect(spotlightTurnOrigin(origin.workshopId, origin.turnId, intent.emitted_at_utc)).toEqual(origin);
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope());
  await vi.runAllTimersAsync();
  expect(api.send).toHaveBeenCalledTimes(1);
  expect(api.send).toHaveBeenCalledWith(expect.objectContaining({ title: "Medousa — reply ready", body: "The editable design is ready." }));
});
it("deduplicates completion events and workspace delivery across notification presenters", async () => {
  rememberSpotlightTurn(origin);
  const first = await import("./notifications");
  await Promise.all([first.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope()), first.presentHomeNotification(intent, origin.workshopId)]);
  await vi.runAllTimersAsync();
  vi.resetModules();
  const reopened = await import("./notifications");
  await reopened.presentHomeNotification(intent, origin.workshopId);
  await vi.runAllTimersAsync();
  expect(api.send).toHaveBeenCalledTimes(1);
});
it("retains workshop/session boundaries and ignores previous completions on reusable agent streams", async () => {
  rememberSpotlightTurn({ ...origin, acceptedAt: "2026-10-25T03:00:00Z" });
  const notifications = await import("./notifications");
  await notifications.notifySpotlightTurnCompletion("other-workshop", origin.sessionId, envelope());
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, "other-chat", envelope("completed", "2026-10-25T03:01:00Z"));
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope());
  await vi.runAllTimersAsync(); expect(api.send).not.toHaveBeenCalled();
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope("completed", "2026-10-25T03:01:00Z"));
  await vi.runAllTimersAsync(); expect(api.send).toHaveBeenCalledOnce();
});
it("notifies again for a subsequent request that reuses an agent stream ID", async () => {
  rememberSpotlightTurn(origin); const notifications = await import("./notifications");
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope());
  await vi.runAllTimersAsync();
  rememberSpotlightTurn({ ...origin, acceptedAt: "2026-10-25T03:00:00Z" });
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope("completed", "2026-10-25T03:01:00Z"));
  await vi.runAllTimersAsync(); expect(api.send).toHaveBeenCalledTimes(2);
});
it("respects the Spotlight reply opt-out and OS permission denial", async () => {
  rememberSpotlightTurn(origin); const notifications = await import("./notifications");
  localStorage.setItem(SPOTLIGHT_REPLIES_KEY, "0");
  notifications.registerSpotlightTurn(origin);
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope());
  await vi.runAllTimersAsync(); expect(api.send).not.toHaveBeenCalled();
  expect(api.granted).not.toHaveBeenCalled();
  localStorage.removeItem(SPOTLIGHT_REPLIES_KEY);
  api.granted.mockResolvedValue(false); api.request.mockResolvedValue("denied");
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope());
  await vi.runAllTimersAsync(); expect(api.send).not.toHaveBeenCalled();
});
it("keeps tool errors/checkpoints/cancellation quiet and alerts only when the request actually ends", async () => {
  rememberSpotlightTurn(origin); const notifications = await import("./notifications");
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, { ...envelope(), event: { type: "error", operator_message: "A tool attempt failed" } });
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope("checkpointed"));
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope("cancelled"));
  await vi.runAllTimersAsync(); expect(api.send).not.toHaveBeenCalled();
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope("failed"));
  await vi.runAllTimersAsync();
  expect(api.send).toHaveBeenCalledWith(expect.objectContaining({ title: "Medousa — request stopped" }));
});

it("waits for the delegated result instead of notifying at the handoff checkpoint", async () => {
  rememberSpotlightTurn(origin); const notifications = await import("./notifications");
  notifications.registerSpotlightWorker(origin.workshopId, { ...envelope(), event: { type: "worker_ack", ack_kind: "worker", work_id: "design-worker", text: "Started" } });
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope("checkpointed"));
  await vi.runAllTimersAsync(); expect(api.send).not.toHaveBeenCalled();
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, { ...envelope(), event: { type: "worker_synthesis", work_id: "design-worker", text: "The delegated design is ready." } });
  await vi.runAllTimersAsync();
  await notifications.presentHomeNotification({ ...intent, notification_id: "worker-result:design-worker", subject_id: "design-worker" }, origin.workshopId);
  await vi.runAllTimersAsync();
  await notifications.notifySpotlightTurnCompletion(origin.workshopId, origin.sessionId, envelope());
  await vi.runAllTimersAsync();
  expect(api.send).toHaveBeenCalledTimes(1);
  expect(api.send).toHaveBeenCalledWith(expect.objectContaining({ body: "The delegated design is ready." }));
});
