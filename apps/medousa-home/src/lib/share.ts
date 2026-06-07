import { workDeepLinkUrl } from "$lib/deepLinks";

export async function shareText(
  title: string,
  text: string,
): Promise<"shared" | "copied" | "failed"> {
  const payload = text.trim();
  if (!payload) return "failed";

  if (typeof navigator !== "undefined" && "share" in navigator) {
    try {
      await navigator.share({ title, text: payload });
      return "shared";
    } catch (err) {
      if (err instanceof Error && err.name === "AbortError") return "failed";
    }
  }

  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(payload);
      return "copied";
    } catch {
      return "failed";
    }
  }

  return "failed";
}

export async function shareWorkResult(
  title: string,
  outputText: string,
  cardId: string,
): Promise<"shared" | "copied" | "failed"> {
  const link = workDeepLinkUrl(cardId);
  const body = `${outputText.trim()}\n\n—\n${title}\n${link}`;
  return shareText(title, body);
}
