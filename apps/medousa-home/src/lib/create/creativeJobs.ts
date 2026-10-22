import type { CreativeApi } from "./creativeApi";
import type {
  CreativeJob,
  CreativeJobRequest,
  CreativeProposalDecision,
} from "$lib/types/generated/daemon_api";
import { randomUuid } from "$lib/utils/randomUuid";
import { CreativeRequestError } from "./creativeErrors";
export type CreativeJobView = Omit<CreativeJob, "context">;
const active = new Set(["queued", "dispatching", "running"]);
export function creativeJobs(
  api: Pick<CreativeApi, "createJob" | "jobs" | "job" | "decideJob">,
  id: string,
  onAccepted: () => Promise<void>,
) {
  let job: CreativeJobView | null = null,
    pending: CreativeJobRequest | null = null,
    decision: CreativeProposalDecision | null = null;
  let busy = false,
    error: string | null = null,
    readers = 0,
    timer: ReturnType<typeof setTimeout> | null = null,
    epoch = 0,
    refreshing = false;
  let preparing = false, fresh = false;
  let history: CreativeJobView[] = [];
  const listeners = new Set<() => void>();
  const notify = () => listeners.forEach((fn) => fn());
  const setJob = (value: CreativeJob) => {
    const { context: _, ...view } = value;
    job = view;
    history = [...history.filter(j => j.request.id !== view.request.id), view].slice(-32);
  };
  function schedule() {
    if (timer) clearTimeout(timer);
    timer =
      readers && job && active.has(job.status)
        ? setTimeout(() => void refresh(), 2000)
        : null;
  }
  async function refresh() {
    if (fresh || pending) return;
    if (refreshing || busy) {
      schedule();
      return;
    }
    const generation = epoch;
    refreshing = true;
    try {
      let value: CreativeJob | undefined;
      if (job && active.has(job.status)) value = await api.job(id, job.request.id);
      else {
        const values = (await api.jobs(id)).jobs.sort((a, b) => a.created_at.localeCompare(b.created_at));
        if (generation === epoch) history = values.map(({ context: _, ...view }) => view);
        value = values.at(-1);
      }
      if (generation === epoch && value) {
        setJob(value);
        error = null;
      }
    } catch (e) {
      if (generation === epoch)
        error = e instanceof Error ? e.message : String(e);
    } finally {
      refreshing = false;
      notify();
      // A view can close and reopen while its initial list is still in flight.
      // Discard that response, then restore the newly retained view's job.
      if (generation !== epoch && readers && !busy) void refresh();
      else schedule();
    }
  }
  async function submit() {
    if (busy || !pending) return;
    busy = true;
    error = null;
    notify();
    try {
      setJob(await api.createJob(id, structuredClone(pending)));
      pending = null;
    } catch (e) {
      // A definite rejection never admitted work. Unknown/lost receipts keep
      // their exact identity; users can recapture after a revision conflict.
      if (e instanceof CreativeRequestError && e.rejected) pending = null;
      error = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      busy = false;
      notify();
      schedule();
    }
  }
  return {
    get preparing() { return preparing; },
    get thread() { return job ? history.filter(j => j.session_id === job!.session_id) : []; },
    newConversation() {
      if (busy || pending || decision || (job && active.has(job.status))) throw new Error("Finish the current request first");
      epoch++; fresh = true; job = null; error = null; notify();
    },
    get job() {
      return job;
    },
    get busy() {
      return busy;
    },
    get error() {
      return error;
    },
    get pending() {
      return pending !== null;
    },
    get pendingDecision() {
      return decision;
    },
    get executing() {
      return Boolean(job && active.has(job.status));
    },
    subscribe(fn: () => void) {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    retain() {
      readers++;
      if (readers === 1) void refresh();
      let released = false;
      return () => {
        if (released) return;
        released = true;
        readers--;
        if (!readers) {
          epoch++;
          if (timer) clearTimeout(timer);
          timer = null;
        }
      };
    },
    refresh,
    async start(prompt: string, revision: number, selectedIds: string[], capture?: () => Promise<string>, refineProposal = false) {
      if (busy || pending || decision || (job && active.has(job.status)))
        throw new Error("Finish the current request first");
      if (!prompt.trim() || !selectedIds.length)
        throw new Error(
          "Select something on the canvas and describe the change",
        );
      if (new TextEncoder().encode(prompt.trim()).length > 16 * 1024)
        throw new Error("Shorten this request to 16 KiB or less");
      if (selectedIds.length > 1000)
        throw new Error("Select fewer than 1,001 objects for this request");
      epoch++;
      const parentId = fresh ? undefined : job?.request.id;
      const capturedIds = [...selectedIds];
      let visualAssetId: string | undefined;
      if (capture) {
        busy = true; preparing = true; error = null; notify();
        try { visualAssetId = await capture(); }
        finally { busy = false; preparing = false; notify(); }
      }
      pending = {
        id: randomUuid(),
        expected_revision: revision,
        selected_ids: capturedIds,
        prompt: prompt.trim(),
        ...(parentId ? { parent_job_id: parentId } : {}),
        ...(visualAssetId ? { visual_asset_id: visualAssetId } : {}),
        refine_proposal: Boolean(parentId && refineProposal),
      };
      fresh = false;
      await submit();
    },
    retry: submit,
    async decide(accept: boolean, revision: number) {
      if (busy || !job) return;
      if (decision && decision.accept !== accept)
        throw new Error(
          "Retry the pending decision before choosing another action",
        );
      epoch++;
      decision ??= { accept, expected_revision: revision };
      busy = true;
      error = null;
      notify();
      try {
        setJob(await api.decideJob(id, job.request.id, { ...decision }));
        decision = null;
        if (job?.status === "accepted") await onAccepted();
      } catch (e) {
        if (e instanceof CreativeRequestError && e.rejected) decision = null;
        error = e instanceof Error ? e.message : String(e);
        throw e;
      } finally {
        busy = false;
        notify();
        schedule();
      }
    },
  };
}
