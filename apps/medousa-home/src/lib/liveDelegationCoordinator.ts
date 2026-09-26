import type { LiveWorkResult } from "$lib/liveWorkResult";

export type CoordinatedLiveResult = { kind: "acknowledgment" } | { kind: "result"; result: LiveWorkResult };

/** Task admission is independent of voice captions and result presentation. */
export class LiveDelegationCoordinator {
  private tail: Promise<unknown> = Promise.resolve();
  private outstanding = 0;

  execute(request: string, run: () => Promise<LiveWorkResult>, signal: AbortSignal): Promise<CoordinatedLiveResult> {
    const normalized = request.toLowerCase().replace(/[.!?,;’']/g, "").replace(/\s+/g, " ").trim();
    // Deliberately narrow: do not interpret corrections, permissions, or stop
    // requests as acknowledgments. This never grants approval or cancels work.
    const acknowledgment = /^(?:okay|ok|yeah|yep|thanks|thank you|no worries|okay no worries|ok no worries|alright|got it)$/.test(normalized);
    if (acknowledgment || (this.outstanding > 0 && !normalized)) {
      return Promise.resolve({ kind: "acknowledgment" });
    }
    this.outstanding += 1;
    const work = this.tail.catch(() => undefined).then(async (): Promise<CoordinatedLiveResult> => {
      if (signal.aborted) throw new Error("Live ended before this request started.");
      return { kind: "result", result: await run() };
    }).finally(() => { this.outstanding -= 1; });
    this.tail = work;
    return work;
  }
}
