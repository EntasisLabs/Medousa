import { daemonUnary } from "$lib/daemon/contractClient";
import type {
  CreativeJob, CreativeJobRequest, CreativeJobListResponse,
  CreativeAsset, CreativeAssetData, CreativeCreateRequest, CreativeDocument,
  CreativeHistoryRequest, CreativeListResponse, CreativeMutationResponse,
  CreativeProposal, CreativeProposalDecision, CreativeProposalListResponse,
  CreativeProposalRequest, CreativeTransaction, CreativeDeleteResponse, CreativeSelectionContext,
} from "$lib/types/generated/daemon_api";

/** Bind once to a workshop. A later active-workshop change cannot redirect a save. */
export function creativeApi(executionRuntimeId: string) {
  if (!executionRuntimeId.trim()) throw new Error("A workshop is required for creative projects");
  const scope = executionRuntimeId.trim();
  return {
    scope,
    createJob: (id: string, request: CreativeJobRequest) => daemonUnary<CreativeJob>("create.projects.by_id.jobs.post", { id }, request, scope),
    jobs: (id: string) => daemonUnary<CreativeJobListResponse>("create.projects.by_id.jobs.get", { id }, undefined, scope),
    job: (id: string, jobId: string) => daemonUnary<CreativeJob>("create.projects.by_id.jobs.by_job_id.get", { id, job_id: jobId }, undefined, scope),
    decideJob: (id: string, jobId: string, request: CreativeProposalDecision) => daemonUnary<CreativeJob>("create.projects.by_id.jobs.by_job_id.decision.post", { id, job_id: jobId }, request, scope),
    list: (cursor?: string, limit = 50) => daemonUnary<CreativeListResponse>("create.projects.get", {}, undefined, scope, { limit: String(limit), ...(cursor ? { cursor } : {}) }),
    create: (request: CreativeCreateRequest) => daemonUnary<CreativeDocument>("create.projects.post", {}, request, scope),
    get: (id: string) => daemonUnary<CreativeMutationResponse>("create.projects.by_id.get", { id }, undefined, scope),
    transact: (id: string, request: CreativeTransaction) => daemonUnary<CreativeMutationResponse>("create.projects.by_id.transactions.post", { id }, request, scope),
    history: (id: string, request: CreativeHistoryRequest) => daemonUnary<CreativeMutationResponse>("create.projects.by_id.history.post", { id }, request, scope),
    delete: (id: string, expectedRevision: number) => daemonUnary<CreativeDeleteResponse>("create.projects.by_id.delete.post", { id }, { expected_revision: expectedRevision }, scope),
    importAsset: (id: string, dataBase64: string) => daemonUnary<CreativeAsset>("create.projects.by_id.assets.post", { id }, { data_base64: dataBase64 }, scope),
    asset: (id: string, assetId: string) => daemonUnary<CreativeAssetData>("create.projects.by_id.assets.by_asset_id.get", { id, asset_id: assetId }, undefined, scope),
    promoteMedia: (id: string, sessionId: string, mediaId: string) => daemonUnary<CreativeAsset>("create.projects.by_id.promote_media.post", { id }, { session_id: sessionId, media_id: mediaId }, scope),
    context: (id: string, selectedIds: string[]) => daemonUnary<CreativeSelectionContext>("create.projects.by_id.context.post", { id }, { selected_ids: selectedIds }, scope),
    proposals: (id: string) => daemonUnary<CreativeProposalListResponse>("create.projects.by_id.proposals.get", { id }, undefined, scope),
    propose: (id: string, request: CreativeProposalRequest) => daemonUnary<CreativeProposal>("create.projects.by_id.proposals.post", { id }, request, scope),
    decide: (id: string, proposalId: string, request: CreativeProposalDecision) => daemonUnary<CreativeMutationResponse>("create.projects.by_id.proposals.by_proposal_id.decision.post", { id, proposal_id: proposalId }, request, scope),
  };
}
export type CreativeApi = ReturnType<typeof creativeApi>;
