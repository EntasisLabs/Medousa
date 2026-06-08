type MermaidApi = typeof import("mermaid").default;

let mermaidInstance: MermaidApi | null = null;

async function ensureMermaid() {
  if (mermaidInstance) return mermaidInstance;
  const mermaid = (await import("mermaid")).default;
  mermaid.initialize({
    startOnLoad: false,
    theme: "dark",
    securityLevel: "strict",
    fontFamily: "inherit",
  });
  mermaidInstance = mermaid;
  return mermaidInstance;
}

/** Render mermaid diagrams inside a markdown container (client only). */
export async function hydrateMermaid(root: HTMLElement): Promise<void> {
  if (typeof window === "undefined") return;

  const blocks = root.querySelectorAll<HTMLElement>("pre.mermaid");
  if (blocks.length === 0) return;

  const mermaid = await ensureMermaid();
  const pending = [...blocks].filter((node) => node.dataset.rendered !== "1");
  if (pending.length === 0) return;

  for (const node of pending) {
    node.dataset.rendered = "1";
  }

  try {
    await mermaid.run({ nodes: pending });
  } catch {
    for (const node of pending) {
      node.classList.add("markdown-mermaid-error");
    }
  }
}
