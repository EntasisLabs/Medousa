import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

/**
 * Tauri assigns a runtime nonce to the inline boot stylesheet and adds that
 * nonce to the packaged app's `style-src` policy. Reuse it for CodeMirror's
 * style-mod stylesheet so the editor's generated layout rules are allowed by
 * the same policy.
 *
 * `HTMLStyleElement.nonce` is intentional: browsers hide nonce values from
 * `getAttribute("nonce")`, while the IDL property remains readable by script.
 */
export function readPackagedStyleNonce(
  doc: Document | undefined = typeof document === "undefined" ? undefined : document,
): string {
  const style = doc?.querySelector<HTMLStyleElement>("style[nonce]");
  return (style?.nonce || style?.getAttribute("nonce") || "").trim();
}

/** CodeMirror extension that authorizes its generated stylesheet in Tauri. */
export function codeMirrorCspExtension(
  doc: Document | undefined = typeof document === "undefined" ? undefined : document,
): Extension {
  const nonce = readPackagedStyleNonce(doc);
  return nonce ? EditorView.cspNonce.of(nonce) : [];
}
