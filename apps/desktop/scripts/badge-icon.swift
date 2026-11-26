#!/usr/bin/env swift
// Composites a high-contrast PR badge (e.g. "#317") onto apps/desktop/assets/icon/icon-1024.png
// and writes a complete multi-resolution macOS .icns bundle via /usr/bin/iconutil.
//
//   swift apps/desktop/scripts/badge-icon.swift "#317" /path/to/out.icns [/path/to/preview-1024.png]

import AppKit
import CoreGraphics
import Foundation

guard CommandLine.arguments.count >= 3 else {
    fputs("Usage: badge-icon.swift <badge-text> <out.icns> [preview-1024.png]\n", stderr)
    exit(1)
}

let label = CommandLine.arguments[1]
let outIcns = URL(fileURLWithPath: CommandLine.arguments[2])
let previewOut = CommandLine.arguments.count >= 4 ? URL(fileURLWithPath: CommandLine.arguments[3]) : nil

let scriptDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let baseIconURL = scriptDir
    .deletingLastPathComponent()
    .appendingPathComponent("assets/icon/icon-1024.png")

guard let dataProvider = CGDataProvider(url: baseIconURL as CFURL),
      let baseImage = CGImage(
          pngDataProviderSource: dataProvider,
          decode: nil,
          shouldInterpolate: true,
          intent: .defaultIntent
      )
else {
    fputs("Failed to load base icon at \(baseIconURL.path)\n", stderr)
    exit(1)
}

let canvasSize = 1024
let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!

func renderMaster(badgeText: String) -> CGImage {
    let ctx = CGContext(
        data: nil,
        width: canvasSize,
        height: canvasSize,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.interpolationQuality = .high
    ctx.draw(baseImage, in: CGRect(x: 0, y: 0, width: canvasSize, height: canvasSize))

    let nsCtx = NSGraphicsContext(cgContext: ctx, flipped: false)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = nsCtx

    let fontSize: CGFloat = badgeText.count <= 4 ? 118 : (badgeText.count <= 6 ? 102 : 88)
    let font = NSFont.monospacedSystemFont(ofSize: fontSize, weight: .bold)
    let attrs: [NSAttributedString.Key: Any] = [
        .font: font,
        .foregroundColor: NSColor(srgbRed: 0.98, green: 0.98, blue: 0.99, alpha: 1.0),
        .kern: -2.0,
    ]
    let attrStr = NSAttributedString(string: badgeText, attributes: attrs)
    let textBounds = attrStr.boundingRect(with: NSSize(width: 900, height: 300), options: [.usesLineFragmentOrigin, .usesFontLeading])

    let padX: CGFloat = 54
    let pillH: CGFloat = 208
    let pillW = max(pillH, min(800, ceil(textBounds.width) + padX * 2))
    let pillRight: CGFloat = 944
    let pillBottom: CGFloat = 76
    let pillRect = NSRect(x: pillRight - pillW, y: pillBottom, width: pillW, height: pillH)
    let radius = pillH / 2

    // Drop shadow under the pill
    ctx.saveGState()
    ctx.setShadow(
        offset: CGSize(width: 0, height: -10),
        blur: 24,
        color: CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 0.48)
    )
    let pillPath = NSBezierPath(roundedRect: pillRect, xRadius: radius, yRadius: radius)
    NSColor(srgbRed: 0.08, green: 0.08, blue: 0.095, alpha: 0.96).setFill()
    pillPath.fill()
    ctx.restoreGState()

    // Subtle concentric inner ring so the dark badge separates cleanly on dark Docks too
    let strokeRect = pillRect.insetBy(dx: 3.0, dy: 3.0)
    let strokePath = NSBezierPath(roundedRect: strokeRect, xRadius: radius - 3.0, yRadius: radius - 3.0)
    strokePath.lineWidth = 5.0
    NSColor(srgbRed: 1.0, green: 1.0, blue: 1.0, alpha: 0.22).setStroke()
    strokePath.stroke()

    let textX = pillRect.midX - textBounds.width / 2 - textBounds.origin.x
    let textY = pillRect.midY - font.capHeight / 2 + font.descender
    attrStr.draw(at: NSPoint(x: round(textX), y: round(textY)))

    NSGraphicsContext.restoreGraphicsState()
    return ctx.makeImage()!
}

func resize(_ image: CGImage, to px: Int) -> Data {
    let ctx = CGContext(
        data: nil,
        width: px,
        height: px,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.interpolationQuality = .high
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: px, height: px))
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    return rep.representation(using: .png, properties: [:])!
}

let master = renderMaster(badgeText: label)
let fm = FileManager.default
let tmpRoot = fm.temporaryDirectory.appendingPathComponent("brocode-icon-\(ProcessInfo.processInfo.processIdentifier)")
let iconsetDir = tmpRoot.appendingPathComponent("icon.iconset")
try fm.createDirectory(at: iconsetDir, withIntermediateDirectories: true)
defer { try? fm.removeItem(at: tmpRoot) }

let slots: [(String, Int)] = [
    ("16x16", 16),
    ("16x16@2x", 32),
    ("32x32", 32),
    ("32x32@2x", 64),
    ("128x128", 128),
    ("128x128@2x", 256),
    ("256x256", 256),
    ("256x256@2x", 512),
    ("512x512", 512),
    ("512x512@2x", 1024),
]

for (name, px) in slots {
    let png = resize(master, to: px)
    try png.write(to: iconsetDir.appendingPathComponent("icon_\(name).png"))
}

if let previewOut {
    try fm.createDirectory(at: previewOut.deletingLastPathComponent(), withIntermediateDirectories: true)
    try resize(master, to: 1024).write(to: previewOut)
}

try fm.createDirectory(at: outIcns.deletingLastPathComponent(), withIntermediateDirectories: true)
let proc = Process()
proc.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
proc.arguments = ["-c", "icns", iconsetDir.path, "-o", outIcns.path]
try proc.run()
proc.waitUntilExit()
guard proc.terminationStatus == 0 else {
    fputs("iconutil exited with status \(proc.terminationStatus)\n", stderr)
    exit(proc.terminationStatus)
}
