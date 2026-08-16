/**
 * Code document / LSP / changes / task service.
 * CodeSourceEditor consumes this instead of calling Forge/daemon directly.
 */

export {
  canStartHumanEditing,
  startHumanEditingSession,
  applyUndertakingSourceWorkspaceEdit,
  getUndertakingSource,
  heartbeatLease,
  humanizeForgeMessage,
  isMissingForgeRoute,
  saveUndertakingSource,
  saveUndertakingSources,
  getUndertakingSourceTree,
  getProjectTasks,
  getProjectTests,
  getForgeChanges,
  getChangesFile,
  restoreChangesFile,
  fetchChanges,
  pullChanges,
  pushChanges,
  syncChanges,
  checkpointChanges,
  getChangesHistory,
  getChangesBlame,
  resolveChangesConflict,
  revertChangesHunk,
  startProjectTaskRun,
  getProjectTaskRun,
  cancelProjectTaskRun,
  getReviewFile,
} from "$lib/forge";

export type {
  ForgeSourceTreeFile,
  ProjectTask,
  ProjectTaskResult,
  ProjectTaskRun,
  ProjectTest,
  ForgeChanges,
  ChangesFileDiff,
  ChangesHistoryEntry,
  ChangesBlameHunk,
  ForgeSourceFile,
} from "$lib/forge";
