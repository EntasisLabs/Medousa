export interface AskJobCompleteActionsRequest {
  writeJournalPath?: string;
  notifyChannel?: string;
}

export interface AskJobCompleteActionsResponse {
  job_id: string;
  ok: boolean;
  message: string;
  journal_path?: string | null;
  notified_channel?: string | null;
}

export interface ArchiveAskJobResponse {
  job_id: string;
  archived: boolean;
  message: string;
}

export interface PendingAskCompletion {
  jobId: string;
  title: string;
}

export function defaultJournalPathForToday(): string {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `journal/${year}-${month}-${day}.md`;
}

export function isAskJobId(id: string): boolean {
  return id.startsWith("medousa-daemon-ask-");
}

/** Matches [`ask_job_session_id`] in the Rust ask job store. */
export function askSessionId(jobId: string): string {
  return `medousa-ask:${jobId.trim()}`;
}

export function isAskSessionId(sessionId: string): boolean {
  return sessionId.startsWith("medousa-ask:");
}

export function askJobIdFromSession(sessionId: string): string | null {
  if (!isAskSessionId(sessionId)) return null;
  return sessionId.slice("medousa-ask:".length);
}
