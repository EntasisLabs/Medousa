/**
 * Vitest / Node harness backend that drives the same Rust `ChatCore` via the
 * `app-chat-dispatch` stdio binary. Keeps a single reconciliation owner.
 *
 * Lives under `vitest/` so the SvelteKit/Vite client bundle never sees Node
 * builtins (`child_process`, `readline`, …).
 */

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { AppChatBackend } from "../src/lib/appChat/client";
import type { ChatCommand, ChatUpdate } from "$lib/types/generated/app_api";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const DEFAULT_BIN = path.join(
  process.env.CARGO_TARGET_DIR ?? path.join(ROOT, "target"),
  "debug",
  "app-chat-dispatch",
);

type Pending = {
  resolve: (update: ChatUpdate) => void;
  reject: (error: Error) => void;
};

export function createNodeDispatchBackend(
  binaryPath = process.env.MEDOUSA_APP_CHAT_DISPATCH ?? DEFAULT_BIN,
): AppChatBackend & { dispose: () => void } {
  const child = spawn(binaryPath, [], {
    stdio: ["pipe", "pipe", "inherit"],
    env: process.env,
  });
  const queue: Pending[] = [];
  const rl = createInterface({ input: child.stdout! });
  rl.on("line", (line) => {
    const pending = queue.shift();
    if (!pending) return;
    try {
      pending.resolve(JSON.parse(line) as ChatUpdate);
    } catch (error) {
      pending.reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
  child.on("error", (error) => {
    while (queue.length) {
      queue.shift()!.reject(error);
    }
  });
  child.on("exit", (code) => {
    const error = new Error(`app-chat-dispatch exited with code ${code ?? "?"}`);
    while (queue.length) {
      queue.shift()!.reject(error);
    }
  });

  const backend: AppChatBackend & { dispose: () => void } = async (command: ChatCommand) => {
    if (!child.stdin) throw new Error("app-chat-dispatch stdin unavailable");
    return await new Promise<ChatUpdate>((resolve, reject) => {
      queue.push({ resolve, reject });
      child.stdin!.write(`${JSON.stringify(command)}\n`);
    });
  };
  backend.dispose = () => {
    rl.close();
    child.kill();
  };
  return backend;
}
