import heicWasmUrl from "@keeratita/heic-converter/wasm?url";
import { guessMimeFromPath } from "$lib/utils/vaultAttachments";

const WEB_SAFE_IMAGE_MIMES = new Set([
  "image/jpeg",
  "image/png",
  "image/gif",
  "image/webp",
]);

const HEIC_MIMES = new Set(["image/heic", "image/heif"]);
const RASTER_CONVERSION_MIMES = new Set([
  "image/avif",
  "image/bmp",
  "image/tiff",
]);

function normalizedMime(mime: string): string {
  const value = mime.split(";", 1)[0]?.trim().toLowerCase() ?? "";
  if (value === "image/jpg") return "image/jpeg";
  if (value === "image/x-ms-bmp") return "image/bmp";
  if (value === "image/x-tiff") return "image/tiff";
  return value;
}

function ascii(bytes: Uint8Array, start: number, length: number): string {
  return String.fromCharCode(...bytes.subarray(start, start + length));
}

export function imageMimeFromBytes(bytes: Uint8Array): string | null {
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  if (
    bytes.length >= 8 &&
    bytes[0] === 0x89 &&
    ascii(bytes, 1, 3) === "PNG" &&
    bytes[4] === 0x0d &&
    bytes[5] === 0x0a &&
    bytes[6] === 0x1a &&
    bytes[7] === 0x0a
  ) {
    return "image/png";
  }
  if (bytes.length >= 6 && ["GIF87a", "GIF89a"].includes(ascii(bytes, 0, 6))) {
    return "image/gif";
  }
  if (
    bytes.length >= 12 &&
    ascii(bytes, 0, 4) === "RIFF" &&
    ascii(bytes, 8, 4) === "WEBP"
  ) {
    return "image/webp";
  }
  if (bytes.length >= 2 && ascii(bytes, 0, 2) === "BM") {
    return "image/bmp";
  }
  if (
    bytes.length >= 4 &&
    (ascii(bytes, 0, 4) === "II*\0" || ascii(bytes, 0, 4) === "MM\0*")
  ) {
    return "image/tiff";
  }
  if (bytes.length >= 12 && ascii(bytes, 4, 4) === "ftyp") {
    const brands = new Set<string>();
    for (let offset = 8; offset + 4 <= Math.min(bytes.length, 64); offset += 4) {
      brands.add(ascii(bytes, offset, 4));
    }
    if (brands.has("avif") || brands.has("avis")) return "image/avif";
    if (
      ["heic", "heix", "hevc", "hevx", "heim", "heis", "mif1", "msf1"].some(
        (brand) => brands.has(brand),
      )
    ) {
      return "image/heic";
    }
  }
  return null;
}

async function effectiveImageMime(file: File): Promise<string> {
  const header = new Uint8Array(await file.slice(0, 64).arrayBuffer());
  const sniffed = imageMimeFromBytes(header);
  if (sniffed) return sniffed;
  const declared = normalizedMime(file.type);
  if (declared.startsWith("image/")) return declared;
  return normalizedMime(guessMimeFromPath(file.name));
}

function withMime(file: File, mime: string): File {
  if (normalizedMime(file.type) === mime) return file;
  return new File([file], file.name, { type: mime, lastModified: file.lastModified });
}

function jpegFileName(name: string): string {
  const trimmed = name.trim() || "photo";
  if (/\.[a-z0-9]+$/i.test(trimmed)) return trimmed.replace(/\.[a-z0-9]+$/i, ".jpg");
  return `${trimmed}.jpg`;
}

async function convertHeicToJpeg(file: File): Promise<File> {
  const { convertHeic, LibheifDecoder } = await import("@keeratita/heic-converter");
  const decoder = new LibheifDecoder({ locateFile: () => heicWasmUrl });
  try {
    const converted = await convertHeic(file, {
      to: "jpeg",
      quality: 0.92,
      decoder,
    });
    return new File([converted], jpegFileName(file.name), {
      type: "image/jpeg",
      lastModified: file.lastModified,
    });
  } finally {
    decoder.free();
  }
}

type DecodedSource = {
  source: CanvasImageSource;
  width: number;
  height: number;
  close: () => void;
};

async function decodeRaster(blob: Blob): Promise<DecodedSource> {
  if (typeof createImageBitmap === "function") {
    const bitmap = await createImageBitmap(blob);
    return {
      source: bitmap,
      width: bitmap.width,
      height: bitmap.height,
      close: () => bitmap.close(),
    };
  }

  const url = URL.createObjectURL(blob);
  try {
    const image = new Image();
    image.src = url;
    await image.decode();
    return {
      source: image,
      width: image.naturalWidth,
      height: image.naturalHeight,
      close: () => URL.revokeObjectURL(url),
    };
  } catch (error) {
    URL.revokeObjectURL(url);
    throw error;
  }
}

function canvasToJpeg(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(new Error("Couldn't convert that image."))),
      "image/jpeg",
      0.92,
    );
  });
}

async function convertRasterToJpeg(file: File): Promise<File> {
  const decoded = await decodeRaster(file);
  try {
    if (decoded.width <= 0 || decoded.height <= 0) {
      throw new Error("That image has invalid dimensions.");
    }
    const canvas = document.createElement("canvas");
    canvas.width = decoded.width;
    canvas.height = decoded.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Image conversion isn't available on this device.");
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(decoded.source, 0, 0);
    const converted = await canvasToJpeg(canvas);
    canvas.width = 0;
    canvas.height = 0;
    return new File([converted], jpegFileName(file.name), {
      type: "image/jpeg",
      lastModified: file.lastModified,
    });
  } finally {
    decoded.close();
  }
}

/** Normalize mobile camera and desktop image files to formats every WebView and model accepts. */
export async function normalizeChatUploadFile(file: File): Promise<File> {
  const mime = await effectiveImageMime(file);
  if (!mime.startsWith("image/")) {
    const fallback = normalizedMime(file.type) || guessMimeFromPath(file.name);
    return withMime(file, fallback);
  }
  if (WEB_SAFE_IMAGE_MIMES.has(mime)) return withMime(file, mime);
  if (HEIC_MIMES.has(mime)) {
    try {
      return await convertHeicToJpeg(withMime(file, mime));
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      throw new Error(`Couldn't convert this iPhone photo. ${detail}`);
    }
  }
  if (RASTER_CONVERSION_MIMES.has(mime)) {
    try {
      return await convertRasterToJpeg(withMime(file, mime));
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      throw new Error(`Couldn't convert this ${mime.replace("image/", "").toUpperCase()} image. ${detail}`);
    }
  }
  throw new Error(
    "That image format isn't supported. Try JPEG, PNG, GIF, WebP, HEIC, HEIF, AVIF, BMP, or TIFF.",
  );
}
