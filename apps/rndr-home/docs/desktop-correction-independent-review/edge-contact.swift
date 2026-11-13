import Foundation
import CoreGraphics
import ImageIO
let r=URL(fileURLWithPath:CommandLine.arguments[1]);let c=CGContext(data:nil,width:1600,height:1390,bitsPerComponent:8,bytesPerRow:6400,space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue)!
c.setFillColor(CGColor(gray:0.5,alpha:1));c.fill(CGRect(x:0,y:0,width:1600,height:1390))
for (name,x,y) in [("edge-renders/light-wide-long-detail.png",0,740),("edge-renders/dark-narrow-long-list.png",1120,740),("edge-renders/light-wide-settings-below-fold.png",0,90),("edge-renders/contrast-narrow-settings-below-fold.png",1120,90),("pixel-crops/contrast-footer-exact.png",0,0)] {
let s=CGImageSourceCreateWithURL(r.appendingPathComponent(name) as CFURL,nil)!;let i=CGImageSourceCreateImageAtIndex(s,0,nil)!;c.draw(i,in:CGRect(x:x,y:y,width:i.width,height:i.height))
}
let d=CGImageDestinationCreateWithURL(r.appendingPathComponent("edge-contact.png") as CFURL,"public.png" as CFString,1,nil)!;CGImageDestinationAddImage(d,c.makeImage()!,nil);assert(CGImageDestinationFinalize(d))
