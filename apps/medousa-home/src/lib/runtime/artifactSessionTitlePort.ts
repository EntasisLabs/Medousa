/** Port so artifacts does not import the chat store. */

let sessionTitlePort: ((sessionId: string) => string) | null = null;

export function setArtifactSessionTitlePort(
  port: ((sessionId: string) => string) | null,
): void {
  sessionTitlePort = port;
}

export function artifactSessionTitle(sessionId: string): string {
  return sessionTitlePort?.(sessionId) ?? sessionId;
}
