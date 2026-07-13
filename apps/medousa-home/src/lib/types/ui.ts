export type BuiltinSurface =
  | "home"
  | "chat"
  | "work"
  | "library"
  | "calendar"
  | "web"
  | "context"
  | "profiles"
  | "workshop"
  | "automations"
  | "peers"
  | "messaging"
  | "runtime"
  | "settings";

/** Built-in surfaces plus agent-defined custom surface ids. */
export type Surface = BuiltinSurface | (string & {});
