// make-icon.swift — draws the tokue app icon at every size macOS asks for.
//
//   swift assets/make-icon.swift <out.iconset>   then   iconutil -c icns <out.iconset>
//
// The mark is assets/logo.svg, drawn here from the same 64-unit geometry
// rather than rasterised once and scaled, so the small sizes stay crisp:
// a green tile (radius 16), a dark ring (radius 15.4, width 8) whose solid
// arc runs clockwise from the top through three quarters, the rest of it
// faint — the quota left, as the menu bar ring shows it. The tile sits on
// Apple's icon grid: an 824-point body in a 1024 canvas, with a soft shadow.

import AppKit

let green = NSColor(srgbRed: 0x3e / 255.0, green: 0xcf / 255.0, blue: 0x8e / 255.0, alpha: 1)
let ink = NSColor(srgbRed: 0x07 / 255.0, green: 0x09 / 255.0, blue: 0x0a / 255.0, alpha: 1)

func icon(pixels: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: pixels, height: pixels)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSGraphicsContext.current?.imageInterpolation = .high

    let scale = CGFloat(pixels) / 1024
    let body = NSRect(x: 100 * scale, y: 100 * scale, width: 824 * scale, height: 824 * scale)
    let unit = body.width / 64 // one unit of the logo's 64-unit grid

    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor(white: 0, alpha: 0.28)
    shadow.shadowOffset = NSSize(width: 0, height: -10 * scale)
    shadow.shadowBlurRadius = 20 * scale
    shadow.set()
    green.setFill()
    NSBezierPath(roundedRect: body, xRadius: 16 * unit, yRadius: 16 * unit).fill()
    NSGraphicsContext.restoreGraphicsState()

    let center = NSPoint(x: body.midX, y: body.midY)
    let radius = 15.4 * unit
    let track = NSBezierPath()
    track.appendArc(withCenter: center, radius: radius, startAngle: 0, endAngle: 360)
    track.lineWidth = 8 * unit
    ink.withAlphaComponent(0.22).setStroke()
    track.stroke()

    // y points up here, so clockwise from the top is 90° down to -180°.
    let arc = NSBezierPath()
    arc.appendArc(withCenter: center, radius: radius, startAngle: 90, endAngle: -180, clockwise: true)
    arc.lineWidth = 8 * unit
    arc.lineCapStyle = .round
    ink.setStroke()
    arc.stroke()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let out = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "AppIcon.iconset")
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    for factor in [1, 2] {
        let name = factor == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        try icon(pixels: points * factor).write(to: out.appendingPathComponent(name))
    }
}
