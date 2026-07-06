export type MediaEmbedProvider = "spotify" | "apple_music";

export interface MediaEmbedConfig {
  provider: MediaEmbedProvider;
  embedUrl: string;
  height: number;
  hidden: boolean;
}

const SPOTIFY_EMBED_HOST = "open.spotify.com/embed/";
const APPLE_EMBED_HOST = "embed.music.apple.com/";

export function parseMediaEmbedProvider(value: unknown): MediaEmbedProvider | null {
  if (value === "spotify" || value === "apple_music") return value;
  return null;
}

export function normalizeSpotifyEmbedUrl(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("https://")) return null;
  if (trimmed.includes(SPOTIFY_EMBED_HOST)) {
    return trimmed.split("?")[0] ?? trimmed;
  }
  const match = trimmed.match(
    /^https:\/\/open\.spotify\.com\/(track|album|playlist|episode|show|artist)\/([a-zA-Z0-9]+)/,
  );
  if (!match) return null;
  return `https://open.spotify.com/embed/${match[1]}/${match[2]}`;
}

export function normalizeAppleMusicEmbedUrl(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("https://")) return null;
  if (trimmed.includes(APPLE_EMBED_HOST)) {
    return trimmed.split("?")[0] ?? trimmed;
  }
  const match = trimmed.match(/^https:\/\/music\.apple\.com\/(.+)/);
  if (!match) return null;
  return `https://embed.music.apple.com/${match[1]}`;
}

export function resolveMediaEmbedConfig(config: Record<string, unknown>): MediaEmbedConfig | null {
  const provider = parseMediaEmbedProvider(config.provider);
  if (!provider) return null;
  const embedUrlRaw =
    (typeof config.embedUrl === "string" && config.embedUrl.trim()) ||
    (typeof config.embed_url === "string" && config.embed_url.trim()) ||
    (typeof config.url === "string" && config.url.trim()) ||
    "";
  if (!embedUrlRaw) return null;

  const embedUrl =
    provider === "spotify"
      ? normalizeSpotifyEmbedUrl(embedUrlRaw)
      : normalizeAppleMusicEmbedUrl(embedUrlRaw);
  if (!embedUrl) return null;

  let height = 352;
  if (typeof config.height === "number" && config.height > 0) {
    height = config.height;
  } else if (provider === "apple_music") {
    height = 150;
  } else if (config.compact === true || config.compact === "true") {
    height = 152;
  }

  const hidden = config.hidden === true || config.hidden === "true";

  return { provider, embedUrl, height, hidden };
}

export function isAllowedMediaEmbedUrl(provider: MediaEmbedProvider, url: string): boolean {
  if (!url.startsWith("https://")) return false;
  if (provider === "spotify") return url.includes(SPOTIFY_EMBED_HOST);
  return url.includes(APPLE_EMBED_HOST);
}
