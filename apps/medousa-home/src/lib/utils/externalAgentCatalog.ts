/** Installed agent runtimes. `muse_code` is distinct from the Muse bot service. */
export const EXTERNAL_AGENTS = [
  { id: "codex", account: "chatgpt", label: "Codex", hint: "ChatGPT-backed agent + chat", cli: "Codex CLI", setup: "install", login: "login", logout: "Sign out" },
  { id: "cursor", account: "cursor", label: "Cursor", hint: "External Cursor agent", cli: "Cursor Agent CLI", setup: "install", login: "login", logout: "Sign out" },
  { id: "hermes", account: "hermes", label: "Hermes", hint: "External Hermes agent", cli: "Hermes Agent CLI", setup: "install", login: "login", logout: "Reconfigure" },
  { id: "claude", account: "claude", label: "Claude Code", hint: "Your Claude Code agent", cli: "Claude Code CLI", setup: "host", login: "host", logout: null },
  { id: "devin", account: "devin", label: "Devin", hint: "Your Devin CLI agent", cli: "Devin CLI", setup: "host", login: "host", logout: null },
  { id: "pi", account: "pi", label: "Pi", hint: "Your Pi coding agent", cli: "Pi CLI", setup: "host", login: "host", logout: null },
  { id: "muse_code", account: "muse_code", label: "Muse Code", hint: "Your Muse Code agent", cli: "Muse Code CLI", setup: "host", login: "host", logout: null },
  { id: "hades", account: "hades", label: "Hades", hint: "Your Hades agent", cli: "Hades CLI", setup: "host", login: "host", logout: null },
] as const;
export type ExternalAgentRuntime = (typeof EXTERNAL_AGENTS)[number]["id"];
export type ExternalAgentAccount = (typeof EXTERNAL_AGENTS)[number]["account"];
export function isExternalRuntime(value: unknown): value is ExternalAgentRuntime {
  return EXTERNAL_AGENTS.some(agent => agent.id === value);
}
export function externalAgent(runtime: ExternalAgentRuntime) {
  return EXTERNAL_AGENTS.find(agent => agent.id === runtime)!;
}
