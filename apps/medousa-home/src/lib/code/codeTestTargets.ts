import type { ProjectTest } from "./codeDocumentService";
export function codeTestTargetSupported(test: ProjectTest): boolean {
  return test.target_kind === "file" || (test.target_kind === "named" && ["cargo", "python", "go"].includes(test.provider ?? ""));
}
/** A file target is one invocation even when it contains many discovered names. */
export function distinctCodeTestTargets(tests: ProjectTest[]): ProjectTest[] {
  const seen = new Set<string>();
  return tests.filter((test) => {
    const key = test.target_kind === "file" ? JSON.stringify([test.task_id, test.path]) : test.id;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
