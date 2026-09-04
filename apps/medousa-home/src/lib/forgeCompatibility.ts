import type { RepositoryInspection } from "$lib/forge";

function folderNameFromPath(path: string): string {
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

/** Local fallback when repository inspect is missing on older daemons. */
export function synthesizeRepositoryInspection(path: string): RepositoryInspection {
  const trimmed = path.trim();
  return {
    repo_id: trimmed,
    path: trimmed,
    display_name: folderNameFromPath(trimmed),
    current_branch: null,
    suggested_base_ref: "main",
    has_commits: true,
    dirty: false,
    changed_files: 0,
    remotes: [],
    existing_projects: [],
    state_explanation:
      "This workshop cannot inspect Git yet. Medousa will still start the project from this folder.",
    trust_explanation:
      "Update medousa_daemon to get branch, dirty-state, and duplicate-project checks.",
  };
}
