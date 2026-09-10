import { forgeFetch } from "$lib/forge";
import { operationPath } from "$lib/daemon";

export type ReviewGitState = { head: string; branch: string; snapshot: string; paths: string[]; base: string; can_branch: boolean };
export async function getReviewGitState(workId: string): Promise<ReviewGitState> {
  return forgeFetch(operationPath("forge.items.by_work_id.changes.git.get", { work_id: workId }));
}
export async function runReviewGitAction(workId: string, action: "commit" | "pull-request", input: {
  lease_id: string; generation: number; snapshot: string; paths?: string[]; message?: string;
  title?: string; body?: string; base?: string; draft?: boolean; branch?: string;
}): Promise<{ head?: string; url?: string; existing?: boolean; warning?: string | null }> {
  return forgeFetch(operationPath(action === "commit" ? "forge.items.by_work_id.changes.commit.post" : "forge.items.by_work_id.changes.pull_request.post", { work_id: workId }), {
    method: "POST", body: JSON.stringify(input),
  });
}
