// Rasterize production Ratatui cells with native macOS text drawing.
import AppKit
import CoreText
let input = URL(fileURLWithPath: CommandLine.arguments[1])
let font: CTFont
if let path = ProcessInfo.processInfo.environment["MUSE_PREVIEW_FONT"],
   let descriptors = CTFontManagerCreateFontDescriptorsFromURL(URL(fileURLWithPath: path) as CFURL) as? [CTFontDescriptor],
   let descriptor = descriptors.first {
    font = CTFontCreateWithFontDescriptor(descriptor, 16, nil)
} else {
    font = CTFontCreateWithName("Menlo" as CFString, 15, nil)
}
let cellW = 9, cellH = 21
func ink(_ value: Any, fallback: NSColor) -> NSColor {
    if let c = value as? [Int], c.count == 3 {
        return NSColor(srgbRed: CGFloat(c[0])/255, green: CGFloat(c[1])/255, blue: CGFloat(c[2])/255, alpha: 1)
    }
    if let s = value as? String, s == "dim" { return .gray }
    return fallback
}
for line in try String(contentsOf: input, encoding: .utf8).split(separator: "\n") {
    let frame = try JSONSerialization.jsonObject(with: Data(line.utf8)) as! [String: Any]
    let cells = frame["cells"] as! [[[Any]]]
    let width = cells[0].count * cellW, height = cells.count * cellH
    let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    for (y,row) in cells.enumerated() { for (x,cell) in row.enumerated() {
        ink(cell[2], fallback: NSColor(srgbRed: 0.09, green: 0.11, blue: 0.15, alpha: 1)).setFill()
        NSRect(x: x*cellW, y: height-(y+1)*cellH, width: cellW, height: cellH).fill()
    }}
    for (y,row) in cells.enumerated() { for (x,cell) in row.enumerated() {
        let text = cell[0] as! String
        if text.isEmpty { continue }
        let weight: CFDictionary = [kCTFontWeightTrait: (cell[3] as? Bool == true ? 0.4 : 0.0)] as CFDictionary
        let desc = CTFontDescriptorCreateWithAttributes([kCTFontTraitsAttribute: weight] as CFDictionary)
        let face = CTFontCreateCopyWithAttributes(font, 16, nil, desc)
        (text as NSString).draw(at: NSPoint(x:x*cellW,y:height-(y+1)*cellH+2), withAttributes:[.font:face,.foregroundColor:ink(cell[1],fallback:.white)])
    }}
    NSGraphicsContext.restoreGraphicsState()
    let output = URL(fileURLWithPath:"\(CommandLine.arguments[2])/\(frame["name"] as! String).png")
    try bitmap.representation(using: .png, properties: [:])!.write(to: output)
    print(output.path)
}
