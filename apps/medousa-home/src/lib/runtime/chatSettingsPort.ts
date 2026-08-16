/** Port so chat does not import the settings store. */

export type ChatSettingsPort = {
  autoOpenWebOnAgentBrowse: () => boolean;
  showEngineDetailsInChat: () => boolean;
};

const unbound: ChatSettingsPort = {
  autoOpenWebOnAgentBrowse: () => false,
  showEngineDetailsInChat: () => false,
};

let ports: ChatSettingsPort | null = null;

export function setChatSettingsPort(next: ChatSettingsPort | null): void {
  ports = next;
}

export function chatSettingsPort(): ChatSettingsPort {
  return ports ?? unbound;
}
