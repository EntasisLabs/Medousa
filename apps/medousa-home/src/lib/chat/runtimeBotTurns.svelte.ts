import { daemonUnary } from "$lib/daemon/contractClient";
import { chat } from "$lib/stores/chat.svelte";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
import type { BotProfile } from "$lib/types/generated/daemon_api";
export interface RuntimeBotJob { botId: string; requestId: string; prompt: string; jobId?: string; status: string; error?: string | null }
interface BotAdmission { ticket: { jobId: string; turnId: string } }
interface BotObservation { status: string; error?: string | null; result?: string | null }
const STORAGE = "medousa-runtime-bot-jobs-v1";
let scope = "";
let jobs = $state<Record<string, RuntimeBotJob>>({});
const requests = new Map<string, Promise<void>>();
const admissions = new Map<string, Promise<BotAdmission | undefined>>();
export function runtimeBotJob(sessionId: string): RuntimeBotJob | null { return scope === chat.workshopScopeId ? jobs[sessionId] ?? null : null; }
export function runtimeBotJobPending(sessionId: string): boolean {
  const job = runtimeBotJob(sessionId);
  return Boolean(job && !["completed", "succeeded", "failed", "cancelled"].includes(job.status));
}
export function loadRuntimeBotJobs() {
  const next = chat.workshopScopeId ?? "";
  if (scope === next) return;
  scope = next; jobs = {}; requests.clear(); admissions.clear();
  if (!scope) return;
  try {
    const stored = JSON.parse(localStorage.getItem(workshopScopedStorageKey(STORAGE, scope)) ?? "{}");
    for (const [sessionId, value] of Object.entries(stored)) {
      if (!value || typeof value !== "object") continue;
      const job = value as RuntimeBotJob;
      if (typeof job.botId !== "string" || typeof job.requestId !== "string" || typeof job.prompt !== "string" || typeof job.status !== "string") continue;
      jobs[sessionId] = job.status === "submitting" && !job.jobId ? {...job,status:"submission_uncertain",error:"Reconnect to the original request using Retry submission."} : job;
    }
  } catch { jobs = {}; }
}
function retain(sessionId: string, job: RuntimeBotJob) {
  jobs = { ...jobs, [sessionId]: job };
  localStorage.setItem(workshopScopedStorageKey(STORAGE, scope), JSON.stringify(jobs));
}
export async function sendRuntimeBotTurn(bot: BotProfile, sessionId: string, prompt: string) {
  loadRuntimeBotJobs();
  if (runtimeBotJobPending(sessionId)) throw new Error("This Bot already has a request in progress.");
  retain(sessionId, { botId: bot.bot_id, requestId: crypto.randomUUID(), prompt, status: "submitting" });
  return admitRuntimeBotTurn(sessionId);
}
export async function admitRuntimeBotTurn(sessionId: string): Promise<BotAdmission | undefined> {
  const job = runtimeBotJob(sessionId);
  if (!job || job.jobId) return;
  const key = `${scope}:${sessionId}`;
  const previous = admissions.get(key);
  if (previous) return previous;
  const workshop = scope;
  const epoch = chat.workshopEpoch;
  retain(sessionId, { ...job, status: "submitting", error: null });
  const pending = daemonUnary<BotAdmission>("bots.ask.post", {}, {
    bot: job.botId, prompt: job.prompt, request_id: job.requestId, session_id: sessionId,
  }).then(async (result) => {
    if (scope !== workshop || chat.workshopScopeId !== workshop || chat.workshopEpoch !== epoch) return;
    retain(sessionId, { ...job, jobId: result.ticket.jobId, status: "transport_pending", error: null });
    if (chat.focusedSessionId === sessionId) await chat.reloadCurrentSession().catch(() => undefined);
    return result;
  }).catch((cause) => {
    if (scope === workshop && chat.workshopScopeId === workshop && chat.workshopEpoch === epoch) {
      retain(sessionId, { ...job, status: "submission_uncertain", error: cause instanceof Error ? cause.message : String(cause) });
    }
    throw cause;
  }).finally(() => { if (admissions.get(key) === pending) admissions.delete(key); });
  admissions.set(key, pending);
  return pending;
}
export async function refreshRuntimeBotTurn(sessionId: string): Promise<void> {
  loadRuntimeBotJobs(); const job = runtimeBotJob(sessionId);
  if (!job?.jobId || !runtimeBotJobPending(sessionId)) return;
  const key = `${scope}:${sessionId}`; const previous = requests.get(key); if (previous) return previous;
  const workshop = scope; const epoch = chat.workshopEpoch;
  const pending = daemonUnary<BotObservation>("bots.ask.by_job_id.get", {job_id:job.jobId}).then(async (result) => {
    if (scope !== workshop || chat.workshopScopeId !== workshop || chat.workshopEpoch !== epoch) return;
    const finished = ["completed", "succeeded", "failed", "cancelled"].includes(result.status);
    retain(sessionId, {...job,status:result.status,error:result.error,prompt:finished ? "" : job.prompt});
    if (finished && chat.focusedSessionId === sessionId) await chat.reloadCurrentSession().catch(() => undefined);
  }).catch((cause) => {
    if (scope === workshop && chat.workshopEpoch === epoch) retain(sessionId, {...job,error:cause instanceof Error ? cause.message : String(cause)});
  }).finally(() => { if (requests.get(key) === pending) requests.delete(key); });
  requests.set(key,pending); return pending;
}
export async function cancelRuntimeBotTurn(sessionId: string) {
  const job = runtimeBotJob(sessionId); if (!job?.jobId) return;
  await daemonUnary("bots.ask.by_job_id.cancel.post",{job_id:job.jobId}); await refreshRuntimeBotTurn(sessionId);
}
