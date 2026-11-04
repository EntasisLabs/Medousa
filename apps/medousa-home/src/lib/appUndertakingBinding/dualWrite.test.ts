import { afterEach, describe, expect, it } from "vitest";
import type { UndertakingBindingCommand, UndertakingBindingUpdate } from "$lib/types/generated/app_api";
import {
  dispatchAppUndertakingBinding,
  installAppUndertakingBindingTestBackend,
} from "./client";

afterEach(() => {
  installAppUndertakingBindingTestBackend(null);
});

describe("dispatchAppUndertakingBinding", () => {
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

    await dispatchAppUndertakingBinding({
      type: "bind_chat",
      session_id: "chat-a",
      group_id: "codePane",
    });

    expect(seen).toHaveLength(1);
    expect(seen[0]).toEqual({
      type: "bind_chat",
      session_id: "chat-a",
      group_id: "codePane",
    });
  });
});
