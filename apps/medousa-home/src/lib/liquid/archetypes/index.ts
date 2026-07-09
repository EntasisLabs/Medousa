/**
 * Liquid UI archetype barrel — importing this registers the vocabulary into both
 * the domain descriptor registry and the Svelte component registry (side effects
 * at import). Consumers import this once before rendering a scene.
 */

// atoms
export { prose } from "./atoms/prose/prose";
export { statusPill } from "./atoms/status_pill/statusPill";
export { media } from "./atoms/media/media";
export { whisper } from "./atoms/whisper/whisper";
export { metadata } from "./atoms/metadata/metadata";
export { button } from "./atoms/button/button";

// molecules
export { callout } from "./molecules/callout/callout";

// layout
export { stack } from "./layout/stack/stack";

// organisms
export { document } from "./organisms/document/document";

// shell (reuse of native chat molecules)
export { thinking } from "./shell/thinking/thinking";
export { toolTrace } from "./shell/tool_trace/toolTrace";
export { presentation } from "./shell/presentation/presentation";
export { chatMedia } from "./shell/chat_media/chatMedia";
