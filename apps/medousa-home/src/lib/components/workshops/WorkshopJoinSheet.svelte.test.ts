/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import WorkshopJoinSheet from "./WorkshopJoinSheet.svelte";
const state = vi.hoisted(() => ({
  joinFromPairLink: vi.fn().mockResolvedValue({ workshopId: "paired-mac" }),
  joinError: null, joinBusy: false, atWorkshopLimit: false, workshops: [],
}));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: state }));
vi.mock("$lib/platform", () => ({ isBrowserWorkshop: () => false, isTauriMobilePlatform: () => false }));
vi.mock("$lib/mobileNavigation", () => ({ registerMobileBackHandler: () => () => {} }));
vi.mock("$lib/utils/mobileSheetGestures", () => ({ attachMobileSheetGestures: () => () => {} }));
const invite = "medousa://pair/1.0?a=192.168.1.2:7419&d=mac&t=token&s=signature&n=Remote%20Mac";
let component: ReturnType<typeof mount>;
afterEach(async () => {
  if (component) await unmount(component);
  document.body.replaceChildren();
  state.joinFromPairLink.mockClear();
});
it("lets an opened invite be named before joining", async () => {
  const onClose = vi.fn();
  component = mount(WorkshopJoinSheet, { target: document.body, props: { open: true, initialPairLink: invite, variant: "desktop", onClose } });
  flushSync();
  expect(state.joinFromPairLink).not.toHaveBeenCalled();
  expect(document.querySelector<HTMLTextAreaElement>("textarea")!.value).toBe(invite);
  const name = document.querySelector<HTMLInputElement>("#workshop-pair-name")!;
  expect(name.placeholder).toBe("Remote Mac");
  name.value = "  Studio Mac  "; name.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  [...document.querySelectorAll("button")].find((button) => button.textContent?.includes("Join workshop"))!.click();
  await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
  expect(state.joinFromPairLink).toHaveBeenCalledWith(invite, { daemonUrl: "http://192.168.1.2:7419", workshopName: "Studio Mac" });
});
it("uses the advertised name when left blank and allows cancelling without pairing", async () => {
  const onClose = vi.fn();
  component = mount(WorkshopJoinSheet, { target: document.body, props: { open: true, initialPairLink: invite, variant: "desktop", onClose } });
  flushSync();
  [...document.querySelectorAll("button")].find((button) => button.textContent?.includes("Cancel"))!.click();
  expect(state.joinFromPairLink).not.toHaveBeenCalled();
  expect(onClose).toHaveBeenCalled();
  [...document.querySelectorAll("button")].find((button) => button.textContent?.includes("Join workshop"))!.click();
  await vi.waitFor(() => expect(state.joinFromPairLink).toHaveBeenCalled());
  expect(state.joinFromPairLink).toHaveBeenCalledWith(invite, { daemonUrl: "http://192.168.1.2:7419", workshopName: undefined });
});
