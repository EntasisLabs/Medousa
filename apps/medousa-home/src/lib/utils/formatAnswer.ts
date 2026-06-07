export type AnswerStateTone = "success" | "warning" | "error" | "primary" | "muted";

export interface AnswerStateDisplay {
  label: string;
  tone: AnswerStateTone;
}

export function formatAnswerState(state: string | null | undefined): AnswerStateDisplay | null {
  if (!state?.trim()) return null;

  switch (state) {
    case "verified":
      return { label: "Verified", tone: "success" };
    case "failed":
    case "verification_failed":
      return { label: "Unverified", tone: "error" };
    case "needs_input":
      return { label: "Needs input", tone: "primary" };
    case "final_pending":
      return { label: "Wrapping up", tone: "warning" };
    default:
      return {
        label: state.replaceAll("_", " "),
        tone: "muted",
      };
  }
}

export function answerStateBadgeClass(tone: AnswerStateTone): string {
  switch (tone) {
    case "success":
      return "variant-soft-success";
    case "error":
      return "variant-soft-error";
    case "warning":
      return "variant-soft-warning";
    case "primary":
      return "variant-soft-primary";
    default:
      return "variant-soft-surface";
  }
}

export function answerStateTextClass(tone: AnswerStateTone): string {
  switch (tone) {
    case "success":
      return "text-success-400";
    case "error":
      return "text-error-400";
    case "warning":
      return "text-warning-400";
    case "primary":
      return "text-primary-300";
    default:
      return "text-surface-500";
  }
}
