import { daemonUnary as dispatch } from "$lib/daemon/contractClient";
import { publishCreativeChange } from "./creativeWindowSync";
import { creativeRequestError, isCreativeAdmissionBusy } from "./creativeErrors";
import type {
  CreativeJob,
  CreativeJobRequest,
  CreativeJobListResponse,
  CreativeAsset,
  CreativeAssetData,
  CreativeCreateRequest,
  CreativeDocument,
  CreativeHistoryRequest,
  CreativeListResponse,
  CreativeMutationResponse,
  CreativeProposal,
  CreativeProposalDecision,
  CreativeProposalListResponse,
  CreativeProposalRequest,
  CreativeTransaction,
  CreativeDeleteResponse,
  CreativeSelectionContext,
  CreativeVisualContextRequest,
} from "$lib/types/generated/daemon_api";

/** Bind once to a workshop. A later active-workshop change cannot redirect a save. */
export function creativeApi(workshopId: string) {
  if (!workshopId.trim())
    throw new Error("A workshop is required for creative projects");
  const scope = workshopId.trim();
  // Client registry scopes must not be passed as daemon runtime overrides.
  const send = async <T>(
    id: Parameters<typeof dispatch>[0],
    params: Record<string, string>,
    body: unknown,
    query?: Record<string, string>,
  ): Promise<T> => {
    const delays = [150, 350, 700];
    for (let attempt = 0; ; attempt++) {
      try {
        const result = await dispatch<T>(id, params, body, null, query, scope);
        if (params.id && (id.endsWith(".post") || id.endsWith(".put"))) publishCreativeChange(scope, params.id);
        if (id === "create.projects.post" && typeof (result as { id?: unknown })?.id === "string") publishCreativeChange(scope, (result as { id: string }).id);
        return result;
      } catch (error) {
        // Startup restores several independent reads. Retain the loading view
        // during brief admission contention; writes keep their explicit retry.
        if (id.endsWith(".get") && isCreativeAdmissionBusy(error) && attempt < delays.length) {
          await new Promise((resolve) => setTimeout(resolve, delays[attempt]));
          continue;
        }
        throw creativeRequestError(error);
      }
    }
  };
  return {
    scope,
    createJob: (id: string, request: CreativeJobRequest) =>
      send<CreativeJob>("create.projects.by_id.jobs.post", { id }, request),
    jobs: (id: string) =>
      send<CreativeJobListResponse>(
        "create.projects.by_id.jobs.get",
        { id },
        undefined,
      ),
    job: (id: string, jobId: string) =>
      send<CreativeJob>(
        "create.projects.by_id.jobs.by_job_id.get",
        { id, job_id: jobId },
        undefined,
      ),
    decideJob: (id: string, jobId: string, request: CreativeProposalDecision) =>
      send<CreativeJob>(
        "create.projects.by_id.jobs.by_job_id.decision.post",
        { id, job_id: jobId },
        request,
      ),
    list: (cursor?: string, limit = 50) =>
      send<CreativeListResponse>("create.projects.get", {}, undefined, {
        limit: String(limit),
        ...(cursor ? { cursor } : {}),
      }),
    create: (request: CreativeCreateRequest) =>
      send<CreativeDocument>("create.projects.post", {}, request),
    get: (id: string) =>
      send<CreativeMutationResponse>(
        "create.projects.by_id.get",
        { id },
        undefined,
      ),
    transact: (id: string, request: CreativeTransaction) =>
      send<CreativeMutationResponse>(
        "create.projects.by_id.transactions.post",
        { id },
        request,
      ),
    history: (id: string, request: CreativeHistoryRequest) =>
      send<CreativeMutationResponse>(
        "create.projects.by_id.history.post",
        { id },
        request,
      ),
    delete: (id: string, expectedRevision: number) =>
      send<CreativeDeleteResponse>(
        "create.projects.by_id.delete.post",
        { id },
        { expected_revision: expectedRevision },
      ),
    importAsset: (id: string, dataBase64: string) =>
      send<CreativeAsset>(
        "create.projects.by_id.assets.post",
        { id },
        { data_base64: dataBase64 },
      ),
    captureSelection: (id: string, dataBase64: string, context: CreativeVisualContextRequest) =>
      send<CreativeAsset>("create.projects.by_id.assets.post", { id }, { data_base64: dataBase64, context }),
    asset: (id: string, assetId: string) =>
      send<CreativeAssetData>(
        "create.projects.by_id.assets.by_asset_id.get",
        { id, asset_id: assetId },
        undefined,
      ),
    promoteMedia: (id: string, sessionId: string, mediaId: string) =>
      send<CreativeAsset>(
        "create.projects.by_id.promote_media.post",
        { id },
        { session_id: sessionId, media_id: mediaId },
      ),
    context: (id: string, selectedIds: string[]) =>
      send<CreativeSelectionContext>(
        "create.projects.by_id.context.post",
        { id },
        { selected_ids: selectedIds },
      ),
    proposals: (id: string) =>
      send<CreativeProposalListResponse>(
        "create.projects.by_id.proposals.get",
        { id },
        undefined,
      ),
    propose: (id: string, request: CreativeProposalRequest) =>
      send<CreativeProposal>(
        "create.projects.by_id.proposals.post",
        { id },
        request,
      ),
    decide: (
      id: string,
      proposalId: string,
      request: CreativeProposalDecision,
    ) =>
      send<CreativeMutationResponse>(
        "create.projects.by_id.proposals.by_proposal_id.decision.post",
        { id, proposal_id: proposalId },
        request,
      ),
  };
}
export type CreativeApi = ReturnType<typeof creativeApi>;
