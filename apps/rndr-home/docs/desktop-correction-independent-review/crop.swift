import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers
let args=CommandLine.arguments
let src=CGImageSourceCreateWithURL(URL(fileURLWithPath:args[1]) as CFURL,nil)!
let img=CGImageSourceCreateImageAtIndex(src,0,nil)!
print("source",img.width,img.height)
let crop=img.cropping(to:CGRect(x:0,y:560,width:480,height:90))!
print("crop",crop.width,crop.height)
let dst=CGImageDestinationCreateWithURL(URL(fileURLWithPath:args[2]) as CFURL,UTType.png.identifier as CFString,1,nil)!
CGImageDestinationAddImage(dst,crop,nil);assert(CGImageDestinationFinalize(dst))
