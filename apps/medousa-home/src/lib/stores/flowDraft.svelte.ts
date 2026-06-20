import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
import type { FlowComposerDraft } from "$lib/types/workflow";
import { emptyFlowDraft } from "$lib/types/workflow";

export class FlowDraftStore {
  pendingRefs = $state<ToolHistorySliceRef[]>([]);
  openComposer = $state(false);
  seedDraft = $state<FlowComposerDraft>(emptyFlowDraft());

  queuePromotion(refs: ToolHistorySliceRef[], draft?: Partial<FlowComposerDraft>) {
    this.pendingRefs = refs;
    this.seedDraft = { ...emptyFlowDraft(), ...draft };
    this.openComposer = true;
  }

  consumePendingRefs(): ToolHistorySliceRef[] {
    const refs = [...this.pendingRefs];
    this.pendingRefs = [];
    return refs;
  }

  clear() {
    this.pendingRefs = [];
    this.openComposer = false;
    this.seedDraft = emptyFlowDraft();
  }
}

export const flowDraft = new FlowDraftStore();
