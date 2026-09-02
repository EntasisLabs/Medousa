/**
 * Read one of the style nonces Tauri injects into packaged app HTML.
 * Browsers may hide nonce values from `getAttribute`, so prefer the IDL field.
 */
export function readPackagedStyleNonce(
  doc: Document | undefined = typeof document === "undefined" ? undefined : document,
): string {
  const style = doc?.querySelector<HTMLStyleElement>("style[nonce]");
  return (style?.nonce || style?.getAttribute("nonce") || "").trim();
}

/**
 * Stamp runtime-created style elements with Tauri's packaged CSP nonce.
 *
 * Libraries such as xterm create their theme and geometry styles while their
 * synchronous `open()` call runs, but do not expose a nonce option. Keep the
 * interception scoped to that call so unrelated DOM creation is unaffected.
 */
export function withPackagedStyleNonce<T>(
  callback: () => T,
  doc: Document | undefined = typeof document === "undefined" ? undefined : document,
): T {
  const nonce = readPackagedStyleNonce(doc);
  if (!doc || !nonce) return callback();

  const ownDescriptor = Object.getOwnPropertyDescriptor(doc, "createElement");
  const createElement = doc.createElement;
  const createElementWithNonce = ((...args: unknown[]) => {
    const element = Reflect.apply(createElement, doc, args) as HTMLElement;
    const tagName = typeof args[0] === "string" ? args[0].toLowerCase() : "";
    if (tagName === "style") {
      (element as HTMLStyleElement).nonce = nonce;
    }
    return element;
  }) as Document["createElement"];

  Object.defineProperty(doc, "createElement", {
    configurable: true,
    writable: true,
    value: createElementWithNonce,
  });

  try {
    return callback();
  } finally {
    if (ownDescriptor) {
      Object.defineProperty(doc, "createElement", ownDescriptor);
    } else {
      Reflect.deleteProperty(doc, "createElement");
    }
  }
}
