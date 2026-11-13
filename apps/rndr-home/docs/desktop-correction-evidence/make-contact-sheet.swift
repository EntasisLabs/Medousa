// Offline evidence composition only. Tiles six unchanged PNGs; no OCR/overlays.
import Foundation
import CoreGraphics
import ImageIO
let root = URL(fileURLWithPath: CommandLine.arguments[1])
let names = ["dark-wide-650-preferences.png", "dark-narrow-650-preferences.png",
             "light-wide-650-accounts.png", "light-narrow-650-basement.png",
             "high-contrast-wide-650-mcp.png", "high-contrast-narrow-650-preferences.png"]
let context = CGContext(data: nil, width: 1600, height: 1950, bitsPerComponent: 8,
                        bytesPerRow: 6400, space: CGColorSpaceCreateDeviceRGB(),
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
for (i, name) in names.enumerated() {
    let source = CGImageSourceCreateWithURL(root.appendingPathComponent(name) as CFURL, nil)!
    let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
    context.draw(image, in: CGRect(x: i % 2 == 0 ? 0 : 1120,
                                 y: 1950 - (i / 2 + 1) * 650,
                                 width: image.width, height: image.height))
}
let out = URL(fileURLWithPath: CommandLine.arguments[2])
let destination = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(destination, context.makeImage()!, nil)
precondition(CGImageDestinationFinalize(destination))
