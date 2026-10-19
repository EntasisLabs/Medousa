/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const mocks = vi.hoisted(() => ({ forge: vi.fn(), hosted: vi.fn(), refresh: vi.fn(), discover: vi.fn() }));
vi.mock("$lib/code/hostedReviews.svelte", () => ({ hostedReviews: {
  providers: [{ id: "github", label: "GitHub", operations: ["inbox"] }], accountProvider: "github", connectionId: "account", folder: "review_requested", nextCursor: null, error: null, loading: false,
  reviews: [{ title: "Hosted fix", state: "open", reference: { provider_id: "github", host: "github.com", repository: "team/repo", native_id: "1" }, updated_at: "2026-10-19" }],
  reset: vi.fn(), discover: mocks.discover, refresh: mocks.refresh,
} }));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: {
  items: [{ id: "local", title: "Local candidate", human_phase: "review", allowed_actions: { review: { allowed: true }, apply: { allowed: false } }, target: {}, updated_at: "2026-10-19" }], loading: false, error: null, refreshList: vi.fn(), startPolling: vi.fn(),
} }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { tabs: [], openCodeReview: mocks.forge, openHostedReview: mocks.hosted } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { activeTab: null } }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { workshopScopeId: "local" } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { openShellSidebarView: vi.fn() } }));
import ReviewsPanel from "./ReviewsPanel.svelte";
let component: ReturnType<typeof mount> | null = null;
async function settle() { for (let i = 0; i < 8; i++) { await Promise.resolve(); flushSync(); } }
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); vi.clearAllMocks(); });
it("keeps Forge and GitHub visible together and routes each to its own review page", async () => {
  component = mount(ReviewsPanel, { target: document.body, props: { rail: true } }); await settle();
  const forge = document.querySelector<HTMLElement>("[aria-label='Forge reviews']")!;
  const github = document.querySelector<HTMLElement>("[aria-label='GitHub reviews']")!;
  expect(forge.textContent).toContain("Local candidate"); expect(github.textContent).toContain("Hosted fix");
  forge.querySelector<HTMLButtonElement>(".queue-row")!.click(); await settle();
  github.querySelector<HTMLButtonElement>(".queue-row")!.click(); await settle();
  expect(mocks.forge).toHaveBeenCalledWith("local", "Local candidate");
  expect(mocks.hosted).toHaveBeenCalledWith(expect.objectContaining({ provider_id: "github", native_id: "1" }), "Hosted fix", "account");
  forge.querySelector<HTMLButtonElement>(".source-heading")!.click(); await settle();
  expect(forge.querySelector(".queue-row")).toBeNull(); expect(github.textContent).toContain("Hosted fix");
});
it("filters both sources without replacing the source sections", async () => {
  component = mount(ReviewsPanel, { target: document.body, props: { rail: true } }); await settle();
  const filter = [...document.querySelectorAll<HTMLButtonElement>("nav button")].find(b => b.textContent === "Reviewed")!;
  filter.click(); await settle();
  expect(document.querySelector("[aria-label='Forge reviews']")).not.toBeNull();
  expect(document.querySelector("[aria-label='GitHub reviews']")).not.toBeNull();
  expect(document.querySelectorAll(".queue-row")).toHaveLength(0);
});
