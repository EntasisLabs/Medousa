/**
 * Liquid UI — binding port (hexagonal).
 *
 * The domain depends on this interface; daemon adapters implement it. The core
 * never imports `daemon.ts` directly. Data-down (`read`/`subscribe`), events-up
 * writes flow through `write`.
 */

import type { Binding } from "../core/scene";

export interface BindingWriteResult {
  revisionHash: string;
}

export interface BindingPort {
  /** Resolve a binding to its normalized data shape. */
  read(binding: Binding): Promise<unknown>;
  /** Persist a change (two-way binding). Optional: not all sources are writable. */
  write?(
    binding: Binding,
    delta: unknown,
    ifMatchHash?: string,
  ): Promise<BindingWriteResult>;
  /** Subscribe to live updates. Returns an unsubscribe function. */
  subscribe?(binding: Binding, onData: (data: unknown) => void): () => void;
}

/** Registry of binding adapters keyed by `BindingSource`. */
export interface BindingResolver {
  resolve(binding: Binding): BindingPort | null;
}
