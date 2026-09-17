export interface TurnCompletion {
  terminal: true;
  phase: "done" | "error" | "cancelled";
  text: string;
}

/** Bounded completion receipts, independent of active turns and display IDs. */
export class TurnCompletionLedger {
  private receipts = new Map<string, { result: TurnCompletion; expires: number }>();
  constructor(private now = () => Date.now()) {}

  private key(scope: string, session: string, turn: string): string {
    return JSON.stringify([scope, session, turn]);
  }

  record(scope: string, session: string, turn: string, result: TurnCompletion): void {
    if (!scope || !session || !turn) return;
    this.receipts.set(this.key(scope, session, turn), {
      result: { ...result, text: result.text.slice(0, 12000) }, expires: this.now() + 10 * 60 * 1000,
    });
    for (const [key, receipt] of this.receipts) if (receipt.expires <= this.now()) this.receipts.delete(key);
    while (this.receipts.size > 100) this.receipts.delete(this.receipts.keys().next().value!);
  }

  read(scope: string, session: string, turn: string): TurnCompletion | null {
    const key = this.key(scope, session, turn);
    const receipt = this.receipts.get(key);
    if (!receipt) return null;
    if (receipt.expires <= this.now()) { this.receipts.delete(key); return null; }
    return { ...receipt.result };
  }
}

export const turnCompletionLedger = new TurnCompletionLedger();
