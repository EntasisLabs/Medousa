/**
 * Vitest-only: drive ChatCore through the Rust `app-chat-dispatch` binary so
 * production never bundles Node child_process into the SvelteKit client.
 */
import { afterAll, beforeAll } from "vitest";
import { installAppChatTestBackend } from "../src/lib/appChat/client";
import { createNodeDispatchBackend } from "./nodeDispatchBackend";

let dispose: (() => void) | undefined;

beforeAll(() => {
  const backend = createNodeDispatchBackend();
  dispose = () => backend.dispose();
  installAppChatTestBackend(backend);
});

afterAll(() => {
  installAppChatTestBackend(null);
  dispose?.();
  dispose = undefined;
});
