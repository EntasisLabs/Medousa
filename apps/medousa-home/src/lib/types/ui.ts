export type BuiltinSurface =
  | "home"
  | "chat"
  | "work"
  | "library"
  | "web"
  | "context"
  | "profiles"
  | "workshop"
  | "automations"
  | "messaging"
  | "runtime"
  | "settings";

/** Built-in surfaces plus agent-defined custom surface ids. */
export type Surface = BuiltinSurface | (string & {});
