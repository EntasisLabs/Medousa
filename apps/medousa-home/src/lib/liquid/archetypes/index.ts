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
export { chip } from "./atoms/chip/chip";

// molecules
export { callout } from "./molecules/callout/callout";
export { cite } from "./molecules/cite/cite";
export { section } from "./molecules/section/section";
export { chipGroup } from "./molecules/chip_group/chipGroup";
export { card } from "./molecules/card/card";
export { carousel } from "./molecules/carousel/carousel";
export { actionRow } from "./molecules/action_row/actionRow";
export { observability } from "./molecules/observability/observability";

// layout
export { stack } from "./layout/stack/stack";

// organisms
export { document } from "./organisms/document/document";
export { compare } from "./organisms/compare/compare";
export { plan } from "./organisms/plan/plan";
export { timeline } from "./organisms/timeline/timeline";
export { shortlist } from "./organisms/shortlist/shortlist";
export { decision } from "./organisms/decision/decision";
export { brief } from "./organisms/brief/brief";
export { dashboard } from "./organisms/dashboard/dashboard";
export { chart } from "./organisms/chart/chart";
export { report } from "./organisms/report/report";

// shell (reuse of native chat molecules)
export { thinking } from "./shell/thinking/thinking";
export { toolTrace } from "./shell/tool_trace/toolTrace";
export { presentation } from "./shell/presentation/presentation";
export { chatMedia } from "./shell/chat_media/chatMedia";
