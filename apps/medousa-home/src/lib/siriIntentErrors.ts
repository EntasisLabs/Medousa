export type SiriAskFailureCode =
  | "busy"
  | "expired"
  | "authentication"
  | "offline"
  | "workshop_unavailable"
  | "unknown";

export type SiriAskFailure = {
  code: SiriAskFailureCode;
  message: string;
};

export function classifySiriAskFailure(error: unknown): SiriAskFailure {
  const detail = error instanceof Error ? error.message : String(error);
  const normalized = detail.toLowerCase();

  if (/expired|already consumed|receipt/.test(normalized)) {
    return {
      code: "expired",
      message: "That Siri request expired. Ask Medousa again to retry.",
    };
  }
  if (/already working|active turn|turn.*active|busy/.test(normalized)) {
    return {
      code: "busy",
      message: "Medousa is already working in this chat. Try again when the turn finishes.",
    };
  }
  if (/401|403|unauthori[sz]ed|authentication|pair it again|no authenticated session/.test(normalized)) {
    return {
      code: "authentication",
      message: "Reconnect the selected workshop in Medousa, then try again.",
    };
  }
  if (/offline|connection refused|timed? out|network|unreachable/.test(normalized)) {
    return {
      code: "offline",
      message: "The selected workshop is offline. Reconnect it and try again.",
    };
  }
  if (/no active workshop|unsupported active workshop|workshop is switching|unavailable/.test(normalized)) {
    return {
      code: "workshop_unavailable",
      message: "The selected workshop isn't available yet. Open Medousa and reconnect it.",
    };
  }
  return {
    code: "unknown",
    message: "Medousa couldn't start that Siri request. The request is ready in the composer.",
  };
}
