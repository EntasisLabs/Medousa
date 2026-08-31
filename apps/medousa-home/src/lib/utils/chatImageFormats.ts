/** Native desktop drops expose paths, so route non-web-safe images through normalization. */
export function nativePathNeedsImageNormalization(path: string): boolean {
  return /\.(?:heic|heif|avif|bmp|tiff?)$/i.test(path.trim());
}
