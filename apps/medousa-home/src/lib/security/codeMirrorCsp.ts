import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { readPackagedStyleNonce } from "./packagedStyleNonce";

export { readPackagedStyleNonce } from "./packagedStyleNonce";

/** Authorize CodeMirror's generated style-mod stylesheet in packaged Tauri. */
export function codeMirrorCspExtension(
  doc: Document | undefined = typeof document === "undefined" ? undefined : document,
): Extension {
  const nonce = readPackagedStyleNonce(doc);
  return nonce ? EditorView.cspNonce.of(nonce) : [];
}
