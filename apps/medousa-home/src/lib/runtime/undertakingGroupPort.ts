let groupIdPort: () => string = () => "default";

export function setUndertakingGroupIdPort(port: () => string): void {
  groupIdPort = port;
}

export function currentUndertakingGroupId(): string {
  return groupIdPort() || "default";
}
