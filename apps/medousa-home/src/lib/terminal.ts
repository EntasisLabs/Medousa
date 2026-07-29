import { invoke } from "@tauri-apps/api/core";

export type TerminalInfo = {
  available: boolean;
  url: string;
  daemon_base_path: string;
  workspace_root: string;
  message: string;
};

export type TerminalSessionSummary = {
  session_id: string;
  cwd: string;
  root_kind: string;
  work_id: string | null;
};

export type TerminalAttachResponse = {
  attach_id: number;
  session_id: string;
};

export type TerminalFrame = {
  attach_id: number;
  rows: { g: string; bold: boolean }[][];
  cursor_row: number;
  cursor_col: number;
};

export async function terminalInfo(): Promise<TerminalInfo> {
  return invoke<TerminalInfo>("terminal_info");
}

export async function terminalSessions(): Promise<TerminalSessionSummary[]> {
  return invoke<TerminalSessionSummary[]>("terminal_sessions");
}

export async function terminalCreate(input: {
  work_id?: string | null;
  cwd?: string | null;
  lease_id?: string | null;
}): Promise<{ session_id?: string } & Record<string, unknown>> {
  return invoke("terminal_create", { input });
}

export async function terminalAttach(sessionId: string): Promise<TerminalAttachResponse> {
  return invoke<TerminalAttachResponse>("terminal_attach", { sessionId });
}

export async function terminalKey(
  attachId: number,
  key: string,
  ctrl: boolean,
  alt: boolean,
  shift: boolean,
): Promise<void> {
  return invoke("terminal_key", { attachId, key, ctrl, alt, shift });
}

export async function terminalResize(
  attachId: number,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("terminal_resize", { attachId, cols, rows });
}

export async function terminalInterrupt(sessionId: string): Promise<unknown> {
  return invoke("terminal_interrupt", { sessionId });
}

export async function terminalDetach(attachId: number): Promise<void> {
  return invoke("terminal_detach", { attachId });
}

export async function terminalSnapshot(attachId: number): Promise<string[]> {
  return invoke<string[]>("terminal_snapshot", { attachId });
}
