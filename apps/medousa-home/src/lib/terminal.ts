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

export type TerminalOutput = {
  attach_id: number;
  data: string;
};

export type TerminalStatus = {
  attach_id: number;
  connected: boolean;
  message: string | null;
};

export type TerminalResizeAck = {
  attach_id: number;
  cols: number;
  rows: number;
};

export type TerminalProtocolError = {
  attach_id: number;
  code: string;
  message: string;
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
  cols?: number;
  rows?: number;
}): Promise<{ session_id?: string } & Record<string, unknown>> {
  return invoke("terminal_create", { input });
}

export async function terminalAttach(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<TerminalAttachResponse> {
  return invoke<TerminalAttachResponse>("terminal_attach", { sessionId, cols, rows });
}

export async function terminalReady(attachId: number): Promise<void> {
  return invoke("terminal_ready", { attachId });
}

export async function terminalWrite(attachId: number, data: string): Promise<void> {
  return invoke("terminal_write", { attachId, data });
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
