/**
 * Liquid Live card — title / subtitle / body on the page; Configure for emoji / meta / points.
 * Click (outside editables) opens the vault card detail sheet when there is detail to show.
 */

import {
  cardHasDetail,
  type CardDetailPayload,
} from "$lib/markdown/liquidEmbeds";
import {
  parseCardFenceBody,
  serializeCardFence,
  type LiquidCardDraft,
} from "$lib/utils/vaultLiquidFence";

function cardBody(raw: string): string {
  const open = /^```card[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

function draftHasDetail(draft: LiquidCardDraft): boolean {
  return (
    cardHasDetail({
      meta: draft.meta,
      chips: [],
      points: draft.points
        .filter((p) => p.label.trim())
        .map((p) => ({ label: p.label.trim(), body: p.value.trim() })),
    }) || Boolean(draft.body.trim())
  );
}

function payloadFromDraft(draft: LiquidCardDraft): CardDetailPayload {
  const payload: CardDetailPayload = {
    id: `live-card-${draft.title || "untitled"}`,
    title: draft.title,
  };
  if (draft.subtitle.trim()) payload.subtitle = draft.subtitle.trim();
  if (draft.emoji.trim()) payload.emoji = draft.emoji.trim();
  if (draft.meta.trim()) payload.meta = draft.meta.trim();
  if (draft.body.trim()) payload.summary = draft.body.trim();
  const points = draft.points
    .map((p) => {
      const label = p.label.trim();
      const body = p.value.trim();
      if (!label) return null;
      return { label, body: body || label };
    })
    .filter((p): p is { label: string; body: string } => p !== null);
  if (points.length) payload.points = points;
  return payload;
}

export type CardSurfaceHandles = {
  destroy: () => void;
};

export function mountCardSurface(
  host: HTMLElement,
  raw: string,
  onChange: (raw: string) => void,
  onOpenCardDetail?: (detail: CardDetailPayload) => void,
): CardSurfaceHandles {
  let draft: LiquidCardDraft = parseCardFenceBody(cardBody(raw));

  const root = document.createElement("div");
  root.className = "vault-live-card liquid-card";
  root.contentEditable = "false";

  const chrome = document.createElement("div");
  chrome.className = "vault-live-card__chrome";

  const emoji = document.createElement("span");
  emoji.className = "vault-live-card__emoji";
  emoji.textContent = draft.emoji || "";
  emoji.hidden = !draft.emoji.trim();

  const expand = document.createElement("button");
  expand.type = "button";
  expand.className = "vault-live-card__expand";
  expand.textContent = "open";
  expand.title = "Open card detail";
  expand.hidden = !draftHasDetail(draft) || !onOpenCardDetail;
  expand.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  expand.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (!onOpenCardDetail || !draftHasDetail(draft)) return;
    onOpenCardDetail(payloadFromDraft(draft));
  });

  const quiet = document.createElement("div");
  quiet.className = "vault-live-quiet-chrome vault-live-card__quiet";

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-card__configure";
  configure.textContent = "more";
  configure.title = "Emoji, meta, points";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "card";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  quiet.append(configure);
  chrome.append(emoji, expand, quiet);

  const title = document.createElement("p");
  title.className = "vault-live-card__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const subtitle = document.createElement("p");
  subtitle.className = "vault-live-card__subtitle";
  subtitle.contentEditable = "true";
  subtitle.spellcheck = true;
  subtitle.dataset.placeholder = "Subtitle";
  subtitle.textContent = draft.subtitle;

  const body = document.createElement("p");
  body.className = "vault-live-card__body";
  body.contentEditable = "true";
  body.spellcheck = true;
  body.dataset.placeholder = "Body";
  body.textContent = draft.body;

  const syncExpand = () => {
    expand.hidden = !draftHasDetail(draft) || !onOpenCardDetail;
  };

  const commit = () => {
    const next: LiquidCardDraft = {
      ...draft,
      title: title.textContent?.replace(/\u00a0/g, " ").trim() ?? "",
      subtitle: subtitle.textContent?.replace(/\u00a0/g, " ").trim() ?? "",
      body: body.textContent?.replace(/\u00a0/g, " ").trim() ?? "",
    };
    if (
      next.title === draft.title &&
      next.subtitle === draft.subtitle &&
      next.body === draft.body
    ) {
      return;
    }
    draft = next;
    syncExpand();
    onChange(serializeCardFence(draft));
  };

  title.addEventListener("blur", commit);
  subtitle.addEventListener("blur", commit);
  body.addEventListener("blur", commit);
  title.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      subtitle.focus();
    }
  });
  subtitle.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      body.focus();
    }
  });

  root.addEventListener("click", (e) => {
    const target = e.target;
    if (!(target instanceof Element)) return;
    if (target.closest("button, a, [contenteditable='true']")) return;
    if (!onOpenCardDetail || !draftHasDetail(draft)) return;
    e.preventDefault();
    e.stopPropagation();
    onOpenCardDetail(payloadFromDraft(draft));
  });

  root.append(chrome, title, subtitle, body);
  host.replaceChildren(root);
  syncExpand();

  return {
    destroy: () => {
      host.replaceChildren();
    },
  };
}
