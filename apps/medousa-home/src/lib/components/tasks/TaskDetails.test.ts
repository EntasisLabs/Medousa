/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const fixture = vi.hoisted(() => ({ liveOutput: "## Full worker result\n\n" + "Evidence paragraph. ".repeat(100) + "\n\nEND_OF_FULL_WORKER_RESULT" }));
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: {
  selectedCardId: "worker", cardDetailError: null, cardActionMessage: null, feed: [], cards: [],
  selectedCardDetail: { card: { id: "worker", title: "Research", column: "done", status_label: "completed", created_at_utc: "", updated_at_utc: "" }, kind: "turn_worker", terminal: true, wrapping_up_reasons: [], associations: { artifact_ids: [], vault_paths: [], locus_node_ids: [] }, result_excerpt: "Short excerpt", get live_output() { return fixture.liveOutput; } },
} }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessionId: "session" } }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: {} }));
vi.mock("$lib/stores/voicePresets.svelte", () => ({ voicePresets: {} }));
vi.mock("$lib/stores/browser.svelte", () => ({ browser: { workCardId: null } }));
vi.mock("$lib/utils/openInBrowser", () => ({ openInBrowser: vi.fn() }));
vi.mock("$lib/share", () => ({ shareWorkResult: vi.fn() }));
vi.mock("$lib/interactiveTurnOptions", () => ({ prepareInteractiveTurnOptions: vi.fn() }));
vi.mock("$lib/daemon", () => ({ archiveAskJob: vi.fn(), approveTurnBudgetRequest: vi.fn(), completeAskJobActions: vi.fn(), denyTurnBudgetRequest: vi.fn(), getJobResult: vi.fn(), lookupArtifact: vi.fn(), createTurnTicket: vi.fn() }));
import TaskDetails from "./TaskDetails.svelte";
let component: ReturnType<typeof mount> | null = null;
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); });
describe("shared task result", () => {
  it.each([false, true])("renders the full worker output in popover=%s", (popover) => {
    component = mount(TaskDetails, { target: document.body, props: { popover, onOpenNote: vi.fn(), onOpenChat: vi.fn() } }); flushSync();
    expect(document.body.textContent).toContain("END_OF_FULL_WORKER_RESULT");
    expect(document.body.textContent).not.toContain("Short excerpt");
  });
});
