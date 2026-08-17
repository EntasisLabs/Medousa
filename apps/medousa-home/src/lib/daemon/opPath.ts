import { OPERATIONS, type OperationId } from "./generatedOps";

/** Percent-encode one path segment. Unreserved characters stay literal. */
function encodePathSegment(value: string): string {
  let encoded = "";
  for (const byte of new TextEncoder().encode(value)) {
    if (
      (byte >= 0x41 && byte <= 0x5a) ||
      (byte >= 0x61 && byte <= 0x7a) ||
      (byte >= 0x30 && byte <= 0x39) ||
      byte === 0x2d ||
      byte === 0x2e ||
      byte === 0x5f ||
      byte === 0x7e
    ) {
      encoded += String.fromCharCode(byte);
    } else {
      encoded += `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
    }
  }
  return encoded;
}

export function expandPath(
  template: string,
  params: Record<string, string> = {},
): string {
  if (template.includes("?")) {
    throw new Error("query text must not be embedded in a path template");
  }
  let path = template;
  for (const [name, value] of Object.entries(params)) {
    const splat = `{*${name}}`;
    const needle = `{${name}}`;
    const encoded = encodePathSegment(value);
    if (path.includes(splat)) {
      path = path.split(splat).join(encoded);
    } else if (path.includes(needle)) {
      path = path.split(needle).join(encoded);
    } else {
      throw new Error(`path template missing parameter ${name}`);
    }
  }
  if (path.includes("{")) {
    throw new Error("path template has unbound parameters");
  }
  return path;
}

export function operationPath(
  id: OperationId,
  params: Record<string, string> = {},
): string {
  return expandPath(OPERATIONS[id].path, params);
}
