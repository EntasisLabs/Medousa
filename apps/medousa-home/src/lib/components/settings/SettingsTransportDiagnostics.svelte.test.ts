/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import SettingsTransportDiagnostics from "./SettingsTransportDiagnostics.svelte";

const api = vi.hoisted(() => ({ read: vi.fn(), copy: vi.fn() }));
vi.mock("$lib/daemon/client", () => ({ getWorkshopTransportDiagnostics: api.read }));
vi.mock("$lib/utils/vaultClipboard", () => ({ copyTextToClipboard: api.copy }));
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.resetAllMocks(); });
async function settle() { await new Promise((resolve) => setTimeout(resolve, 0)); flushSync(); }

it("reads evidence only when opened or refreshed and copies the captured snapshot", async () => {
  const snapshot = [{ sequence: 1, phase: "dial", outcome: "connected", elapsed_ms: 40, peer: "public-peer", path: "relay", rtt_ms: 30 }];
  api.read.mockResolvedValue(snapshot); api.copy.mockResolvedValue(true);
  component = mount(SettingsTransportDiagnostics, { target: document.body });
  await settle();
  expect(api.read).not.toHaveBeenCalled();
  const details = document.querySelector("details")!;
  details.open = true; details.dispatchEvent(new Event("toggle"));
  await settle();
  expect(document.body.textContent).toContain("RTT 30 ms");
  const buttons = [...document.querySelectorAll("button")];
  buttons.find((button) => button.textContent === "Copy diagnostics")!.click();
  await settle();
  expect(api.copy).toHaveBeenCalledWith(JSON.stringify(snapshot, null, 2));
  expect(document.body.textContent).toContain("Copied diagnostics.");
  const reads = api.read.mock.calls.length;
  await settle();
  expect(api.read).toHaveBeenCalledTimes(reads);
  buttons.find((button) => button.textContent === "Refresh")!.click();
  await settle();
  expect(api.read).toHaveBeenCalledTimes(reads + 1);
});
