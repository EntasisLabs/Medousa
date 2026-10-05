import type { ProjectTask } from "$lib/code/codeDocumentService";
export const COMMAND_KINDS = ["run", "build", "test", "verify"] as const;
export function commandRoot(task: ProjectTask): string { return (task.root ?? ".").replace(/\\/g, "/").replace(/^\.\//, "").replace(/\/$/, "") || "."; }
/** Only roots supplied by runtime discovery can define a package. */
export function currentCommandRoot(tasks: ProjectTask[], path: string): string | null {
  const file = path.replace(/\\/g, "/");
  return [...new Set(tasks.map(commandRoot))]
    .filter((root) => root === "." || file === root || file.startsWith(`${root}/`))
    .sort((a, b) => b.length - a.length)[0] ?? null;
}
export function contextualCommands(tasks: ProjectTask[], path: string): ProjectTask[] {
  const root = currentCommandRoot(tasks, path);
  return root === null ? [] : tasks.filter((task) => commandRoot(task) === root);
}
export function commandKindLabel(kind: string): string {
  return kind === "verify" ? "Check" : kind === "run" ? "Run" : kind === "build" ? "Build" : kind === "test" ? "Test" : "Other";
}
export function suggestedCommand(tasks: ProjectTask[], path: string): ProjectTask | null {
  const ranked = contextualCommands(tasks, path).filter((task) => task.available !== false)
    .sort((a, b) => (b.default_rank ?? 0) - (a.default_rank ?? 0));
  if (ranked.length > 1 && (ranked[0].default_rank ?? 0) === (ranked[1].default_rank ?? 0)) return null;
  return ranked[0] ?? null;
}
export function filterCommands(tasks: ProjectTask[], query: string): ProjectTask[] {
  const words = query.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  return tasks.filter((task) => {
    const text = [task.label, commandRoot(task), task.provider, ...task.argv].join(" ").toLocaleLowerCase();
    return words.every((word) => text.includes(word));
  }).sort((a, b) => {
    const order = (kind: string) => { const index = COMMAND_KINDS.indexOf(kind as typeof COMMAND_KINDS[number]); return index < 0 ? 4 : index; };
    return order(a.kind) - order(b.kind) || (b.default_rank ?? 0) - (a.default_rank ?? 0) || a.label.localeCompare(b.label) || commandRoot(a).localeCompare(commandRoot(b));
  });
}
