export type BuiltinSurface =
  | "home"
  | "chat"
  | "work"
  | "code"
  | "library"
  | "notes"
  | "files"
  | "artifacts"
  | "calendar"
  | "web"
  | "context"
  | "map"
  | "profiles"
  | "workshop"
  | "automations"
  | "peers"
  | "messaging"
  | "runtime"
  | "settings";

/** Built-in surfaces plus agent-defined custom surface ids. */
export type Surface = BuiltinSurface | (string & {});
