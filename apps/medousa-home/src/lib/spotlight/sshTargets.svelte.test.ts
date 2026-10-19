/** @vitest-environment happy-dom */
import { expect, it, vi } from "vitest";
import { SpotlightSshTargets } from "./sshTargets.svelte";
const targets = [{ target_id: "ssh-nas", name: "NAS", host: "nas.local", port: 22, username: "ops", agent_access: true }];
it("drops late results after close or workshop switch", async () => {
  let finish!: (value: typeof targets) => void;
  const old = new SpotlightSshTargets("old", () => new Promise((resolve) => { finish = resolve; }));
  const pending = old.refresh(); old.dispose();
  const current = new SpotlightSshTargets("new", async () => targets);
  await current.refresh(); finish(targets); await pending;
  expect(old.targets).toEqual([]); expect(current.targets).toEqual(targets);
});
it("reports load errors and lets the user retry", async () => {
  const list = vi.fn().mockRejectedValueOnce(new Error("Workshop offline")).mockResolvedValue(targets);
  const data = new SpotlightSshTargets("mac", list);
  await data.refresh(); expect(data.error).toBe("Workshop offline"); expect(data.loading).toBe(false);
  await data.refresh(); expect(data.error).toBeNull(); expect(data.targets).toEqual(targets);
});
