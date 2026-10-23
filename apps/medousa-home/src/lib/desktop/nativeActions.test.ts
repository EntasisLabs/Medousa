import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

const native = fileURLToPath(new URL("../../../src-tauri/src/", import.meta.url));
const request = readFileSync(join(native, "daemon/window_request.rs"), "utf8");
const allowlist = request.slice(request.indexOf("pub fn supports_command"));
const commands = [...allowlist.matchAll(/"([a-z_]+)"/g)].map(match => match[1]);
const sources = ["terminal.rs", "browser_host.rs", ...readdirSync(join(native, "daemon")).filter(name => name.endsWith(".rs")).map(name => `daemon/${name}`)]
  .map(path => readFileSync(join(native, path), "utf8"));
const handler = readFileSync(join(native, "lib.rs"), "utf8");

it("every migrated action injects native window authority before bypassing the workshop guard", () => {
  for (const command of commands) {
  expect(handler).toContain(`::${command},`);
  const declaration = new RegExp(`pub (?:async )?fn ${command}\\(`);
  const source = sources.find(source => declaration.test(source));
  expect(source, `${command} must have a native implementation`).toBeDefined();
  const start = source!.search(declaration);
  const signature = source!.slice(start, source!.indexOf("{", start));
  // Reading a user-dropped image is explicitly a device operation, with no daemon routing.
  if (command === "media_read_image_path") continue;
  if (command.startsWith("ssh_")) {
    expect(signature).toContain("tauri::Webview");
    continue;
  }
  expect(signature).toContain("WorkshopRequest");
  expect(signature).not.toMatch(/State<'_, (?:crate::daemon::)?DaemonState>/);
  }
});

it("covers complete conversation workflows while retaining the guard for unmigrated setup", () => {
  expect(commands).toEqual(expect.arrayContaining([
    "turn_create", "interactive_turn_send", "session_get_history", "session_cancel_active_turn",
    "agents_prompt", "agents_approve_permission", "agents_fulfill_secret_request",
    "turn_budget_approve", "media_upload", "media_read", "artifact_write",
    "runtime_patch_tui_preferences", "identity_list_profiles", "composer_stt_transcribe",
    "chatgpt_oauth_request", "browser_host_resume_session", "ssh_action",
  ]));
  expect(commands).not.toContain("local_inference_stream_download");
  expect(commands).not.toContain("set_daemon_url");
});


it("shell IPC supports a main window that also hosts embedded browser webviews", () => {
  const routes = ["daemon/mod.rs", "daemon/contract_bridge.rs", "daemon/code.rs", "daemon/forge.rs", "terminal.rs", "code_lsp_transport.rs", "desktop_windows.rs"];
  for (const route of routes) {
    const source = readFileSync(join(native, route), "utf8");
    const signatures = [...source.matchAll(/#\[tauri::command\]\s+pub (?:async )?fn [^{]+/g)].map(match => match[0]);
    for (const signature of signatures) expect(signature, route).not.toContain("WebviewWindow");
  }
  expect(readFileSync(join(native, "active_workshop.rs"), "utf8")).not.toContain("tauri::WebviewWindow");
});
