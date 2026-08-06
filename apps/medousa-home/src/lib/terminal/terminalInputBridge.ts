/**
 * Bridge so Code can send selected text into the active workshop terminal pane.
 */

export type TerminalInputHandler = {
  workId: string | null;
  write: (text: string) => void;
};

const handlers = new Set<TerminalInputHandler>();

export function registerTerminalInputHandler(handler: TerminalInputHandler): () => void {
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}

/** Prefer a handler matching workId; otherwise the most recently registered. */
export function writeToTerminal(text: string, workId?: string | null): boolean {
  const payload = text.endsWith("\n") ? text : `${text}\n`;
  const preferred =
    (workId
      ? [...handlers].reverse().find((handler) => handler.workId === workId)
      : null) ?? [...handlers].at(-1);
  if (!preferred) return false;
  preferred.write(payload);
  return true;
}
