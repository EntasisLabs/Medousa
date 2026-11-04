import { afterEach, describe, expect, it, vi } from "vitest";
import type { UndertakingBindingCommand, UndertakingBindingUpdate } from "$lib/types/generated/app_api";
import {
  dualWriteUndertakingBinding,
  installAppUndertakingBindingTestBackend,
} from "./client";

afterEach(() => {
  installAppUndertakingBindingTestBackend(null);
});

describe("dualWriteUndertakingBinding", () => {
  it("dispatches through the installed test backend", async () => {
    const seen: UndertakingBindingCommand[] = [];
    installAppUndertakingBindingTestBackend(async (command) => {
      seen.push(command);
      const update: UndertakingBindingUpdate = {
        type: "snapshot",
        snapshot: {
          workshopScopeId: "personal",
          revision: 1,
          contexts: [],
        },
      };
      return update;
    });

    dualWriteUndertakingBinding({
      type: "bind_chat",
      session_id: "chat-a",
      group_id: "codePane",
    });

    await vi.waitFor(() => expect(seen).toHaveLength(1));
    expect(seen[0]).toEqual({
      type: "bind_chat",
      session_id: "chat-a",
      group_id: "codePane",
    });
  });

  it("swallows backend failures", () => {
    installAppUndertakingBindingTestBackend(async () => {
      throw new Error("core unavailable");
    });
    expect(() =>
      dualWriteUndertakingBinding({ type: "clear_active", group_id: "main" }),
    ).not.toThrow();
  });
});
