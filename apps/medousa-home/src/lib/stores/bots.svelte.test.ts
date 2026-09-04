/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
import { BotStore, type BotStoreApi } from "$lib/stores/bots.svelte";
import type {
  BotOpenResponse,
  BotProfile,
} from "$lib/types/generated/daemon_api";

function profile(overrides: Partial<BotProfile> = {}): BotProfile {
  return {
    schema_version: 1,
    bot_id: "bot_0123456789abcdef0123456789abcdef",
    owner_profile_id: "profile-home",
    display_name: "Ada",
    role_description: "Connects the concepts",
    avatar_ref: "🧭",
    primary_manuscript_id: "teacher",
    additional_manuscript_ids: [],
    memory_scope_id: "bot_0123456789abcdef0123456789abcdef",
    default_mode: null,
    primary_session_id: "session-ada",
    archived: false,
    revision: 1,
    created_at: "2026-09-03T00:00:00Z",
    updated_at: "2026-09-03T00:00:00Z",
    ...overrides,
  };
}

function openResponse(bot: BotProfile): BotOpenResponse {
  return {
    bot,
    binding: {
      bot_id: bot.bot_id,
      session_id: bot.primary_session_id ?? "session-ada",
      kind: "primary",
      bot_revision_at_bind: bot.revision,
      created_at: bot.created_at,
    },
  };
}

function api(overrides: Partial<BotStoreApi> = {}): BotStoreApi {
  const bot = profile();
  return {
    list: vi.fn(async () => ({ bots: [bot] })),
    create: vi.fn(async () => openResponse(bot)),
    update: vi.fn(async () => bot),
    setArchived: vi.fn(async () => bot),
    duplicate: vi.fn(async () => openResponse(bot)),
    open: vi.fn(async () => openResponse(bot)),
    ...overrides,
  };
}

describe("BotStore", () => {
  it("resolves the durable Bot from its primary conversation", async () => {
    const store = new BotStore(api());

    await store.refresh();

    expect(store.forSession("session-ada")?.display_name).toBe("Ada");
    expect(store.forSession("another-session")).toBeNull();
  });

  it("does not publish a stale workshop response after switching", async () => {
    let resolveList: ((value: { bots: BotProfile[] }) => void) | undefined;
    const pending = new Promise<{ bots: BotProfile[] }>((resolve) => {
      resolveList = resolve;
    });
    const store = new BotStore(api({ list: vi.fn(() => pending) }));

    const refresh = store.refresh();
    store.activateWorkshopScope("workshop-two");
    resolveList?.({ bots: [profile()] });
    await refresh;

    expect(store.bots).toEqual([]);
  });

  it("uses the current profile revision for edits", async () => {
    const updated = profile({ display_name: "Grace", revision: 4 });
    const update = vi.fn(async () => updated);
    const store = new BotStore(api({ update }));
    const current = profile({ revision: 3 });

    await store.update(current, {
      display_name: "Grace",
      role_description: null,
      avatar_ref: "✨",
      primary_manuscript_id: "teacher",
      additional_manuscript_ids: [],
      default_mode: null,
    });

    expect(update).toHaveBeenCalledWith(
      current.bot_id,
      expect.objectContaining({ expected_revision: 3, display_name: "Grace" }),
    );
    expect(store.bots[0]?.display_name).toBe("Grace");
  });
});
