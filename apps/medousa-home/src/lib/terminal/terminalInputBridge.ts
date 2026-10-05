/**
 * Bridge so Code can send selected text into the active workshop terminal pane.
 */

export type TerminalInputHandler = {
  workId: string | null;
  sessionId?: () => string;
  ready?: () => boolean;
  write: (text: string) => void;
};

const handlers = new Set<TerminalInputHandler>();

export function registerTerminalInputHandler(handler: TerminalInputHandler): () => void {
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}

/** A scoped send must never fall through to another project or process. */
export function writeToTerminal(text: string, workId?: string | null, sessionId?: string | null): boolean {
  const payload = text.endsWith("\n") ? text : `${text}\n`;
  const preferred = [...handlers].reverse().find((handler) =>
    (!workId || handler.workId === workId) && (!sessionId || handler.sessionId?.() === sessionId)
  );
  if (!preferred || preferred.ready?.() === false) return false;
  preferred.write(payload);
  return true;
}
