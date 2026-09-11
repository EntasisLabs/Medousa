/** Fixed learning continuations, not tool names or executable model payloads. */
export const TEACHER_ACTIONS = {
  "teacher.try_example": "Let me try one small practice example of the concept we are discussing. Wait for my attempt before showing the solution.",
  "teacher.show_model": "Show me one worked example of the concept we are discussing and explain why each step works.",
  "teacher.check_understanding": "Check my understanding of the concept we are discussing with one prediction or transfer question. Wait for my answer before giving feedback.",
} as const;

export type TeacherActionIntent = keyof typeof TEACHER_ACTIONS;

/** Unknown reserved IDs fail closed; ordinary chat intents keep their behavior. */
export function resolveTeacherIntent(intent: string | null): string | null {
  if (!intent?.startsWith("teacher.")) return intent;
  if (!Object.prototype.hasOwnProperty.call(TEACHER_ACTIONS, intent)) return null;
  const action = intent as TeacherActionIntent;
  return `The learner chose ${action}. ${TEACHER_ACTIONS[action]}`;
}
