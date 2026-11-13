import Foundation
import Vision
import AppKit
for path in CommandLine.arguments.dropFirst() {
 print("FILE \(path)")
 let req = VNRecognizeTextRequest()
 req.recognitionLevel = .accurate
 do {
 try VNImageRequestHandler(url: URL(fileURLWithPath:path)).perform([req])
 for ob in req.results ?? [] {
 let r=ob.boundingBox
 print(String(format:"%.3f %.3f %.3f %.3f ",r.minX,r.minY,r.width,r.height)+(ob.topCandidates(1).first?.string ?? ""))
 }
 } catch { print(error) }
}
