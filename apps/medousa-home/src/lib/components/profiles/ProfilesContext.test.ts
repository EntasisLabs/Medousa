/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { ProfileShelfEntry } from "$lib/types/profileShelf";
import { blobForShelfEntry, buildIdentityFieldLayout } from "$lib/utils/identityField";
import { buildProfileShelfEntries } from "$lib/utils/profileShelf";

const fixture = vi.hoisted(() => ({ remember: vi.fn() }));
vi.mock("$lib/daemon", () => ({ rememberIdentityFact: fixture.remember }));
vi.mock("$lib/stores/userProfiles.svelte", () => ({ userProfiles: { resolvedUserId: "user:test" } }));
import ProfilesFocusCard from "./ProfilesFocusCard.svelte";
import ProfilesTeachDialog from "./ProfilesTeachDialog.svelte";

let component: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (component) unmount(component);
  component = null;
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("saved profile context", () => {
  it("shows a full saved fact when selected from a shortened field label", () => {
    const summary = "I work across distributed systems, Rust, and clinical integrations.";
    const context = { graph_depth_used: 1, flattened_claims: [{ claim_id: "fact", summary, confidence: 0.9 }] };
    const layout = buildIdentityFieldLayout(context, "Tom");
    const blob = layout.blobs.find((item) => item.entry?.id === "claim:fact")!;
    expect(blob.label.length).toBeLessThan(summary.length);
    expect(buildProfileShelfEntries(context)[0].subtitle).toBe(summary);
    component = mount(ProfilesFocusCard, { target: document.body, props: { blob, portrait: layout.portrait, onClose: () => {} } });
    flushSync();
    expect(document.querySelector("h2")?.textContent).toBe(summary);
    expect(document.querySelector(".profiles-focus-voice")).toBeNull();
  });

  it("shows preference values without changing their case or punctuation", () => {
    const entry: ProfileShelfEntry = { id: "pref:answer_style", kind: "preference", title: "Answer Style", subtitle: "Use Rust/C++ examples; keep API names intact.", searchText: "" };
    component = mount(ProfilesFocusCard, { target: document.body, props: { blob: blobForShelfEntry(entry), portrait: "", onClose: () => {} } });
    flushSync();
    expect(document.querySelector(".profiles-focus-voice")?.textContent).toBe(entry.subtitle);
  });

  it("saves an edited preference to its original key instead of parsing the value as a new note", async () => {
    fixture.remember.mockResolvedValue({ committed: true, requires_confirmation: false, proposal_ids: [], message: "Saved" });
    const entry: ProfileShelfEntry = { id: "pref:answer_style", kind: "preference", title: "Answer Style", subtitle: "Concise", searchText: "", rememberKind: "preference", rememberSubject: "answer_style" };
    component = mount(ProfilesTeachDialog, { target: document.body, props: { open: true, prefill: entry.subtitle, entry, onClose: () => {} } });
    flushSync();
    const input = document.querySelector("textarea")!;
    input.value = "Detailed with examples";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    document.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await vi.waitFor(() => expect(fixture.remember).toHaveBeenCalledWith({ user_id: "user:test", fact_kind: "preference", subject: "answer_style", statement: "Detailed with examples", source: "user_direct" }));
  });
});
