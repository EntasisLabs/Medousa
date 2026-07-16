/**
 * Liquid Live report — narrative + nested figures editable without Build.
 * Resting state mounts the Preview organism; focus reveals reshape chrome.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

export type LiveReportModel = {
  title: string;
  subtitle: string;
  columns: "1" | "2" | "3";
  body: string;
};

function parseKvLines(lines: string[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of lines) {
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

export function parseReportRaw(raw: string): LiveReportModel {
  const open = /^```report[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  const inner =
    open && closeIdx > open[0].length
      ? raw.slice(open[0].length, closeIdx)
      : raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
  const normalized = inner.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const preamble: string[] = [];
  let bodyStart = 0;
  for (let i = 0; i < lines.length; i++) {
    const stripped = (lines[i] ?? "").trim();
    if (!stripped) {
      if (preamble.length > 0) {
        bodyStart = i + 1;
        break;
      }
      continue;
    }
    if (/^[a-zA-Z][a-zA-Z0-9_-]*\s*:/.test(stripped) && !stripped.startsWith("|")) {
      preamble.push(lines[i] ?? "");
      bodyStart = i + 1;
      continue;
    }
    bodyStart = i;
    break;
  }
  const fields = parseKvLines(preamble);
  const columnsRaw = (fields.columns ?? "2").trim();
  const columns =
    columnsRaw === "1" || columnsRaw === "3" ? columnsRaw : "2";
  return {
    title: fields.title ?? "",
    subtitle: fields.subtitle ?? "",
    columns,
    body: lines.slice(bodyStart).join("\n").trim(),
  };
}

export function serializeReportRaw(model: LiveReportModel): string {
  const lines = ["```report"];
  if (model.title.trim()) lines.push(`title: ${model.title.trim()}`);
  if (model.subtitle.trim()) lines.push(`subtitle: ${model.subtitle.trim()}`);
  lines.push(`columns: ${model.columns}`);
  lines.push("");
  if (model.body.trim()) lines.push(model.body.trim());
  lines.push("```");
  return lines.join("\n") + "\n";
}

export type ReportSurfaceHandles = {
  destroy: () => void;
};

export function mountReportSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): ReportSurfaceHandles {
  let model = parseReportRaw(raw);
  let editing = false;
  const root = document.createElement("div");
  root.className = "vault-live-report";
  root.contentEditable = "false";

  const chrome = document.createElement("div");
  chrome.className = "vault-live-report__chrome";

  const colGroup = document.createElement("div");
  colGroup.className = "vault-live-report__cols";
  for (const col of ["1", "2", "3"] as const) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-report__col";
    btn.textContent = `${col} col`;
    btn.setAttribute("aria-pressed", model.columns === col ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (model.columns === col) return;
      model = { ...model, columns: col };
      for (const el of colGroup.querySelectorAll<HTMLButtonElement>(".vault-live-report__col")) {
        el.setAttribute(
          "aria-pressed",
          el.textContent?.startsWith(col) ? "true" : "false",
        );
      }
      onChange(serializeReportRaw(model));
    });
    colGroup.append(btn);
  }

  const editBtn = document.createElement("button");
  editBtn.type = "button";
  editBtn.className = "vault-live-report__edit";
  editBtn.textContent = "Edit narrative";
  editBtn.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  chrome.append(colGroup, editBtn);

  const stage = document.createElement("div");
  stage.className = "vault-live-report__stage";

  const showOrganism = () => {
    editing = false;
    editBtn.textContent = "Edit narrative";
    stage.replaceChildren();
    const mount = document.createElement("div");
    stage.append(mount);
    mountLiquidFence(mount, serializeReportRaw(model), liquidContext);
  };

  const showEditor = () => {
    editing = true;
    editBtn.textContent = "Done";
    unmountLiquidFence(stage);
    stage.replaceChildren();

    const title = document.createElement("input");
    title.className = "vault-live-report__field";
    title.type = "text";
    title.placeholder = "Title";
    title.value = model.title;

    const subtitle = document.createElement("input");
    subtitle.className = "vault-live-report__field";
    subtitle.type = "text";
    subtitle.placeholder = "Subtitle";
    subtitle.value = model.subtitle;

    const body = document.createElement("textarea");
    body.className = "vault-live-report__body";
    body.placeholder = "Narrative and nested ```chart``` fences…";
    body.value = model.body;
    body.rows = Math.min(16, Math.max(6, model.body.split("\n").length + 2));

    const commit = () => {
      model = {
        ...model,
        title: title.value.trim(),
        subtitle: subtitle.value.trim(),
        body: body.value.replace(/\r\n/g, "\n").trim(),
      };
      onChange(serializeReportRaw(model));
    };

    title.addEventListener("change", commit);
    subtitle.addEventListener("change", commit);
    body.addEventListener("change", commit);

    stage.append(title, subtitle, body);
    title.focus();
  };

  editBtn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (editing) {
      const fields = stage.querySelectorAll<HTMLInputElement>(".vault-live-report__field");
      const body = stage.querySelector<HTMLTextAreaElement>(".vault-live-report__body");
      model = {
        ...model,
        title: fields[0]?.value.trim() ?? model.title,
        subtitle: fields[1]?.value.trim() ?? model.subtitle,
        body: body?.value.replace(/\r\n/g, "\n").trim() ?? model.body,
      };
      onChange(serializeReportRaw(model));
      showOrganism();
      return;
    }
    showEditor();
  });

  root.append(chrome, stage);
  host.replaceChildren(root);
  showOrganism();

  return {
    destroy: () => {
      unmountLiquidFence(stage);
      host.replaceChildren();
    },
  };
}
