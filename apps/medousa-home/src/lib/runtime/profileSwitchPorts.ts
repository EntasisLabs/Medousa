/** Ports so userProfiles does not import chat/identity stores. */

export type ProfileSwitchPorts = {
  hasConversation: () => boolean;
  refreshSessions: () => Promise<void>;
  refreshIdentity: (userId: string | null) => Promise<void>;
};

const unbound: ProfileSwitchPorts = {
  hasConversation: () => false,
  refreshSessions: async () => {},
  refreshIdentity: async () => {},
};

let ports: ProfileSwitchPorts | null = null;

export function setProfileSwitchPorts(next: ProfileSwitchPorts | null): void {
  ports = next;
}

export function profileSwitchPorts(): ProfileSwitchPorts {
  return ports ?? unbound;
}
