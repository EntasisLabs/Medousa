/** Path-based hints, shared by hosted and Forge reviews. Each file belongs to one group. */
export type ReviewChangeKind = "code" | "tests" | "ci" | "docs" | "other";
export const reviewChangeKinds: ReviewChangeKind[] = ["code", "tests", "ci", "docs", "other"];
export const changeKindLabels: Record<ReviewChangeKind, string> = { code: "Code", tests: "Tests", ci: "CI", docs: "Docs", other: "Other" };
export interface ChangeFile { path: string; additions?: number | null; deletions?: number | null }
export function reviewChangeKind(path: string): ReviewChangeKind {
  const p = path.replaceAll("\\", "/").toLowerCase();
  if (/(^|\/)(\.github\/(workflows|actions)|\.circleci|\.buildkite|ci)(\/|$)/.test(p) || /(^|\/)(\.gitlab-ci[^/]*|jenkinsfile|azure-pipelines[^/]*|\.travis\.yml)$/.test(p)) return "ci";
  if (/(^|\/)(tests?|__tests__|specs?|fixtures|__snapshots__)(\/|$)/.test(p) || /[._](tests?|specs?)\.[^/]+$/.test(p) || /(^|\/)(tests?|specs?)\.[^/]+$/.test(p) || /(^|\/)test_[^/]+\.[^/]+$/.test(p) || /_test\.[^/]+$/.test(p)) return "tests";
  if (/(^|\/)(docs?|documentation)(\/|$)/.test(p) || /\.(mdx?|rst|adoc)$/.test(p) || /(^|\/)(readme|changelog|license|contributing)(\.[^/]*)?$/.test(p)) return "docs";
  if (/\.(rs|[cm]?[jt]sx?|svelte|vue|py|go|java|kt|swift|[ch](pp|xx)?|cs|rb|php|sh|bash|zsh|sql|graphql|gr|css|scss|sass|less|html|exs?|erl|clj|dart|lua|r|m|mm|zig|wasm)$/.test(p)) return "code";
  return "other";
}
export function typedChanges(files: ChangeFile[]) {
  return reviewChangeKinds.map(kind => {
    const group = files.filter(file => reviewChangeKind(file.path) === kind);
    return { kind, label: changeKindLabels[kind], count: group.length,
      additions: group.reduce((n, f) => n + (f.additions ?? 0), 0), deletions: group.reduce((n, f) => n + (f.deletions ?? 0), 0),
      known: group.every(f => f.additions != null && f.deletions != null) };
  }).filter(group => group.count);
}
