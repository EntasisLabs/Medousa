/**
 * DOM snapshot helpers shared by chart PNG export and vault PDF/Word capture.
 * Kept off the Liquid chart module graph so charts do not import markdown hydrate.
 */

export async function waitForPaint(): Promise<void> {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

function replaceBalancedCssFn(
  input: string,
  fnName: string,
  replacement: string,
): string {
  const openRe = new RegExp(`${fnName}\\s*\\(`, "gi");
  let out = "";
  let last = 0;
  let match: RegExpExecArray | null;
  openRe.lastIndex = 0;
  while ((match = openRe.exec(input))) {
    const start = match.index;
    if (fnName.toLowerCase() === "color") {
      const prev = input.slice(Math.max(0, start - 4), start);
      if (/mix$/i.test(prev)) continue;
      if (start > 0 && /[a-z-]/i.test(input[start - 1] ?? "")) continue;
    }
    let i = start + match[0].length;
    let depth = 1;
    while (i < input.length && depth > 0) {
      const ch = input[i++];
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
    }
    out += input.slice(last, start) + replacement;
    last = i;
    openRe.lastIndex = i;
  }
  return out + input.slice(last);
}

function stripUnsupportedColorFns(css: string): string {
  let out = replaceBalancedCssFn(css, "color-mix", "transparent");
  out = replaceBalancedCssFn(out, "color", "#111827");
  return out;
}

export function scrubUnsupportedColorFunctionsInClone(doc: Document): void {
  for (const sheet of Array.from(doc.styleSheets)) {
    const owner = sheet.ownerNode;
    if (!(owner instanceof HTMLStyleElement)) continue;
    try {
      const text = owner.textContent ?? "";
      if (/color-mix\s*\(|(^|[^a-z-])color\s*\(/.test(text)) {
        owner.textContent = stripUnsupportedColorFns(text);
      }
    } catch {
      /* ignore */
    }
  }

  for (const el of doc.querySelectorAll<HTMLElement | SVGElement>("[style]")) {
    const raw = el.getAttribute("style");
    if (!raw || !/color-mix\s*\(|(^|[^a-z-])color\s*\(/.test(raw)) continue;
    el.setAttribute("style", stripUnsupportedColorFns(raw));
  }

  for (const el of doc.querySelectorAll("svg [fill], svg [stroke], svg [stop-color]")) {
    for (const attr of ["fill", "stroke", "stop-color"]) {
      const raw = el.getAttribute(attr);
      if (!raw || !/color-mix\s*\(|(^|[^a-z-])color\s*\(/.test(raw)) continue;
      el.setAttribute(attr, "#64748b");
    }
  }
}

/**
 * Snapshot a DOM node to a PNG data URL (for Word ImageRun / PDF freeze).
 * Temporarily reveals off-screen export mounts so html2canvas can measure.
 */
export async function snapshotElementToPng(
  el: HTMLElement,
): Promise<{ dataUrl: string; width: number; height: number } | null> {
  const shell = el.closest(".vault-pdf-export-shell") as HTMLElement | null;
  const prevShell = shell
    ? {
        visibility: shell.style.visibility,
        opacity: shell.style.opacity,
        pointerEvents: shell.style.pointerEvents,
      }
    : null;

  if (shell) {
    shell.style.visibility = "visible";
    shell.style.opacity = "0";
    shell.style.pointerEvents = "none";
  }

  try {
    await waitForPaint();
    const attempt = async () => {
      const rect = el.getBoundingClientRect();
      const width = Math.max(1, Math.ceil(el.scrollWidth || rect.width));
      const height = Math.max(1, Math.ceil(el.scrollHeight || rect.height));
      if (width < 2 || height < 2) return null;

      const html2canvas = (await import("html2canvas")).default;
      const canvas = await html2canvas(el, {
        backgroundColor: "#ffffff",
        scale: 2,
        useCORS: true,
        logging: false,
        width,
        height,
        windowWidth: width,
        onclone: (clonedDoc: Document) => {
          scrubUnsupportedColorFunctionsInClone(clonedDoc);
        },
      });
      return {
        dataUrl: canvas.toDataURL("image/png"),
        width: canvas.width,
        height: canvas.height,
      };
    };

    const first = await attempt();
    if (first) return first;
    await new Promise<void>((r) => setTimeout(r, 80));
    return await attempt();
  } catch {
    return null;
  } finally {
    if (shell && prevShell) {
      shell.style.visibility = prevShell.visibility;
      shell.style.opacity = prevShell.opacity;
      shell.style.pointerEvents = prevShell.pointerEvents;
    }
  }
}
