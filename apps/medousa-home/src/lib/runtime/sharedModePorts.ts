/** Port so sharedMode does not import userProfiles. */

export type SharedModePort = {
  reloadUserProfiles: () => Promise<void>;
};

const unbound: SharedModePort = {
  reloadUserProfiles: async () => {},
};

let ports: SharedModePort | null = null;

export function setSharedModePort(next: SharedModePort | null): void {
  ports = next;
}

export function sharedModePort(): SharedModePort {
  return ports ?? unbound;
}
