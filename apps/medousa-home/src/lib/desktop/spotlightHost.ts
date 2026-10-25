import { invoke } from "@tauri-apps/api/core";
import type { SpotlightHost, SpotlightHostRequest, SpotlightSnapshot } from "$lib/spotlight/host";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { openDesktopWindow } from "./windows";

/** Native IPC pins each request to this opening's workshop and checks its reply. */
export function createDesktopSpotlightHost(scope: string, options: { isCurrent?: () => boolean; pending?: (delta: number) => void; close?: () => void } = {}): SpotlightHost {
  const request = <T>(request: SpotlightHostRequest) => invoke<T>("desktop_spotlight_request", { workshopId: scope, request });
  const current = () => options.isCurrent?.() ?? true;
  const closeAfter = async (work: Promise<unknown>) => {
    options.pending?.(1);
    try {
      await work;
      if (current()) { if (options.close) options.close(); else commandSpotlight.closeSpotlight(); }
    } catch (cause) {
      // A shell command may have focused its owner before failing. Restore the
      // error to the still-open launcher, but never resurrect a dismissed one.
      if (current() && commandSpotlight.open) await invoke("desktop_spotlight_restore", { workshopId: scope }).catch(() => {});
      throw cause;
    } finally { options.pending?.(-1); }
  };
  return {
    collect: query => request<SpotlightSnapshot>({ kind: "collect", query }),
    async execute(command, args) {
      if (command.id === "terminal-open") {
        await closeAfter(openDesktopWindow({ kind: "terminal", workshopId: scope })); return;
      }
      if (command.id.startsWith("terminal-ssh:")) {
        await closeAfter(openDesktopWindow({ kind: "terminal", workshopId: scope, sshTargetId: command.id.slice("terminal-ssh:".length) })); return;
      }
      switch (command.id) {
        case "workshop-activity": commandSpotlight.openActivity(); return;
        case "terminal-open-ssh": commandSpotlight.openSshTargets(); return;
        case "workbench.action.showCommands": commandSpotlight.openCommandPalette(); return;
        case "do-resume-spotlight": commandSpotlight.restoreLastQuery(); return;
        case "go-desktop-spotlight": commandSpotlight.closeSpotlight(); return;
      }
      await closeAfter(request({ kind: "execute", token: command.token, args }));
    },
    pin: slot => closeAfter(request({ kind: "pin", slot })),
    openConversation: sessionId => closeAfter(openDesktopWindow({ kind: "chat", workshopId: scope, resourceId: sessionId })),
    async openActivity(target, title) {
      if (target.kind === "chat" || (target.kind === "card" && target.sessionId)) {
        await closeAfter(openDesktopWindow({ kind: "chat", workshopId: scope, resourceId: target.sessionId, title }));
      } else if (target.kind === "project" && !target.review) {
        await closeAfter(openDesktopWindow({ kind: "code", workshopId: scope, resourceId: target.workId, title }));
      } else await closeAfter(request({ kind: "activity", target, title }));
    },
    newAsk: () => closeAfter(request({ kind: "new-ask" })),
    manageConnection: () => closeAfter(request({ kind: "connection" })),
    createFlow: (refs, name) => closeAfter(request({ kind: "create-flow", refs, name })),
    openSsh: targetId => closeAfter(openDesktopWindow({ kind: "terminal", workshopId: scope, sshTargetId: targetId })),
  };
}
