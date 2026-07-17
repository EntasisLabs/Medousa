/**
 * Liquid Live callout — tinted aside editable in place (no Build exile).
 */

const TONES = ["note", "warn", "error", "success"] as const;
export type CalloutTone = (typeof TONES)[number];

export type LiveCalloutModel = {
  tone: CalloutTone;
  title: string;
  body: string;
};

function parseKv(body: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of body.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (key && value) out[key] = value;
  }
  return out;
}

export function parseCalloutRaw(raw: string): LiveCalloutModel {
  const open = /^```callout[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  const body =
    open && closeIdx > open[0].length
      ? raw.slice(open[0].length, closeIdx)
      : raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
  const fields = parseKv(body);
  const toneRaw = (fields.tone ?? "note").toLowerCase();
  const tone = (TONES as readonly string[]).includes(toneRaw)
    ? (toneRaw as CalloutTone)
    : "note";
  return {
    tone,
    title: fields.title ?? "",
    body: fields.body ?? "",
  };
}

export function serializeCalloutRaw(model: LiveCalloutModel): string {
  const lines = ["```callout", `tone: ${model.tone}`];
  if (model.title.trim()) lines.push(`title: ${model.title.trim()}`);
  lines.push(`body: ${model.body.trim() || " "}`);
  lines.push("```");
  return lines.join("\n") + "\n";
}

export type CalloutSurfaceHandles = {
  root: HTMLElement;
  setModel: (model: LiveCalloutModel) => void;
  destroy: () => void;
};

/**
 * Mount an editable callout. Calls `onChange` when tone/title/body settle (blur / tone click).
 */
export function mountCalloutSurface(
  host: HTMLElement,
  initial: LiveCalloutModel,
  onChange: (model: LiveCalloutModel) => void,
): CalloutSurfaceHandles {
  let model = { ...initial };
  const root = document.createElement("aside");
  root.className = "liquid-callout vault-live-callout";
  root.dataset.tone = model.tone;
  root.contentEditable = "false";

  const chrome = document.createElement("div");
  chrome.className = "vault-live-quiet-chrome vault-live-callout__chrome";
  for (const tone of TONES) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-callout__tone";
    btn.dataset.tone = tone;
    btn.textContent = tone;
    btn.setAttribute("aria-pressed", tone === model.tone ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      model = { ...model, tone };
      root.dataset.tone = tone;
      for (const el of chrome.querySelectorAll<HTMLButtonElement>(".vault-live-callout__tone")) {
        el.setAttribute("aria-pressed", el.dataset.tone === tone ? "true" : "false");
      }
      onChange(model);
    });
    chrome.append(btn);
  }

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-callout__configure";
  configure.textContent = "more";
  configure.title = "Configure callout";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "callout";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  chrome.append(configure);

  const title = document.createElement("p");
  title.className = "liquid-callout-title vault-live-callout__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.textContent = model.title;
  title.dataset.placeholder = "Title";

  const body = document.createElement("p");
  body.className = "liquid-callout-body vault-live-callout__body";
  body.contentEditable = "true";
  body.spellcheck = true;
  body.textContent = model.body;
  body.dataset.placeholder = "Write…";

  const commit = () => {
    const next: LiveCalloutModel = {
      tone: model.tone,
      title: title.textContent?.replace(/\u00a0/g, " ").trim() ?? "",
      body: body.textContent?.replace(/\u00a0/g, " ").trim() ?? "",
    };
    if (
      next.tone === model.tone &&
      next.title === model.title &&
      next.body === model.body
    ) {
      return;
    }
    model = next;
    onChange(model);
  };

  title.addEventListener("blur", commit);
  body.addEventListener("blur", commit);
  title.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      body.focus();
    }
  });

  root.append(chrome, title, body);
  host.replaceChildren(root);

  return {
    root,
    setModel: (next) => {
      model = { ...next };
      root.dataset.tone = model.tone;
      title.textContent = model.title;
      body.textContent = model.body;
      for (const el of chrome.querySelectorAll<HTMLButtonElement>(".vault-live-callout__tone")) {
        el.setAttribute(
          "aria-pressed",
          el.dataset.tone === model.tone ? "true" : "false",
        );
      }
    },
    destroy: () => {
      host.replaceChildren();
    },
  };
}
