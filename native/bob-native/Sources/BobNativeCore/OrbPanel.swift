import AppKit
import QuartzCore

/// Layout of the orb near the caret (pure, unit-tested): above and to the right of the caret when there
/// is room on that screen, else below it; near the mouse when no caret is known.
public enum OrbLayout {
    public static let size = CGSize(width: 44, height: 44)

    /// - Parameters: all rects in AppKit coordinates.
    public static func origin(caret: CGRect?, mouse: CGPoint, visibleFrame: CGRect) -> CGPoint {
        var p: CGPoint
        if let c = caret {
            p = CGPoint(x: c.maxX + 6, y: c.maxY + 6)
            if p.y + size.height > visibleFrame.maxY { p.y = c.minY - size.height - 6 }
        } else {
            p = CGPoint(x: mouse.x + 14, y: mouse.y + 14)
        }
        p.x = min(max(p.x, visibleFrame.minX + 4), visibleFrame.maxX - size.width - 4)
        p.y = min(max(p.y, visibleFrame.minY + 4), visibleFrame.maxY - size.height - 4)
        return p
    }
}

/// The dictation orb (VD-3): a small ghost that glows and breathes with the voice while listening, spins a
/// ring while thinking, flashes on done and shows a short message bubble on errors. Click-through, never
/// takes focus.
public final class OrbPanel: NSPanel {
    private let circle = CALayer()
    private let ring = CAShapeLayer()
    private let ghost = CATextLayer()
    private let bubble = NSTextField(wrappingLabelWithString: "")
    private var reduceMotion: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
    public private(set) var state: OrbState = .hidden

    public init() {
        super.init(contentRect: NSRect(origin: .zero, size: OrbLayout.size),
                   styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        level = .statusBar
        ignoresMouseEvents = true
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]

        let view = NSView()
        view.wantsLayer = true
        let s = OrbLayout.size
        circle.frame = CGRect(x: 4, y: 4, width: s.width - 8, height: s.height - 8)
        circle.cornerRadius = (s.width - 8) / 2
        circle.backgroundColor = Palette.midnight.withAlphaComponent(0.92).cgColor
        circle.borderWidth = 2
        circle.borderColor = Palette.ectoplasm.cgColor
        circle.shadowColor = Palette.ectoplasm.cgColor
        circle.shadowRadius = 8
        circle.shadowOpacity = 0.6
        circle.shadowOffset = .zero
        ring.frame = CGRect(origin: .zero, size: s)
        ring.path = CGPath(ellipseIn: CGRect(x: 2, y: 2, width: s.width - 4, height: s.height - 4), transform: nil)
        ring.fillColor = nil
        ring.strokeColor = Palette.ectoplasm.cgColor
        ring.lineWidth = 2
        ring.strokeEnd = 0.3
        ring.lineCap = .round
        ring.isHidden = true
        ghost.string = "👻"
        ghost.fontSize = 18
        ghost.alignmentMode = .center
        ghost.contentsScale = NSScreen.main?.backingScaleFactor ?? 2
        ghost.frame = CGRect(x: 0, y: (s.height - 24) / 2, width: s.width, height: 24)
        view.layer?.addSublayer(circle)
        view.layer?.addSublayer(ring)
        view.layer?.addSublayer(ghost)
        contentView = view

        bubble.font = .systemFont(ofSize: 12)
        bubble.textColor = .white
        bubble.drawsBackground = true
        bubble.backgroundColor = Palette.midnight.withAlphaComponent(0.94)
        bubble.isBordered = false
        bubble.preferredMaxLayoutWidth = 280
        bubble.isHidden = true
        view.addSubview(bubble)
    }

    public override var canBecomeKey: Bool { false }
    public override var canBecomeMain: Bool { false }

    /// Show the orb in `state` near `caretAX` (AX coordinates) or the mouse.
    public func show(_ state: OrbState, text: String?, caretAX: Rect?) {
        self.state = state
        guard state != .hidden else { hideOrb(); return }
        let color: NSColor = state == .error ? Palette.pumpkin : Palette.ectoplasm
        circle.borderColor = color.cgColor
        circle.shadowColor = color.cgColor
        ring.strokeColor = color.cgColor
        ring.removeAllAnimations()
        circle.removeAllAnimations()
        ring.isHidden = state != .thinking
        if state == .thinking && !reduceMotion {
            let spin = CABasicAnimation(keyPath: "transform.rotation.z")
            spin.fromValue = 0
            spin.toValue = -2 * Double.pi
            spin.duration = 0.9
            spin.repeatCount = .infinity
            ring.add(spin, forKey: "spin")
        }
        if state == .done && !reduceMotion {
            let pop = CABasicAnimation(keyPath: "transform.scale")
            pop.fromValue = 1.25
            pop.toValue = 1
            pop.duration = 0.25
            circle.add(pop, forKey: "pop")
        }
        ghost.string = state == .done ? "✓" : state == .error ? "!" : "👻"
        layout(text: state == .error ? text : nil, caretAX: caretAX)
        orderFrontRegardless()
    }

    /// Breathe with the voice (0...1) while listening.
    public func setLevel(_ level: Float) {
        guard state == .listening else { return }
        let scale = reduceMotion ? 1 : 1 + CGFloat(level) * 0.22
        CATransaction.begin()
        CATransaction.setAnimationDuration(0.08)
        circle.setAffineTransform(CGAffineTransform(scaleX: scale, y: scale))
        circle.shadowRadius = 6 + CGFloat(level) * 14
        CATransaction.commit()
    }

    public func hideOrb() {
        state = .hidden
        ring.removeAllAnimations()
        circle.setAffineTransform(.identity)
        orderOut(nil)
    }

    private func layout(text: String?, caretAX: Rect?) {
        let primaryHeight = Double(NSScreen.screens.first?.frame.height ?? 0)
        let caret = caretAX.map { CoordinateConverter.appKitRect(fromAX: $0, primaryScreenHeight: primaryHeight) }
            .map { CGRect(x: $0.x, y: $0.y, width: $0.width, height: $0.height) }
        let anchor = caret.map { CGPoint(x: $0.midX, y: $0.midY) } ?? NSEvent.mouseLocation
        let screen = NSScreen.screens.first { $0.frame.contains(anchor) } ?? NSScreen.main
        let vf = screen?.visibleFrame ?? .zero
        let origin = OrbLayout.origin(caret: caret, mouse: NSEvent.mouseLocation, visibleFrame: vf)
        var size = OrbLayout.size
        if let text, !text.isEmpty {
            bubble.stringValue = " \(text) "
            bubble.isHidden = false
            let fit = bubble.sizeThatFits(NSSize(width: 280, height: 200))
            bubble.frame = NSRect(x: OrbLayout.size.width + 4, y: (OrbLayout.size.height - fit.height) / 2,
                                  width: fit.width, height: fit.height)
            bubble.wantsLayer = true
            bubble.layer?.cornerRadius = 6
            size.width += fit.width + 8
        } else {
            bubble.isHidden = true
        }
        var o = origin
        o.x = min(o.x, vf.maxX - size.width - 4)
        setFrame(NSRect(origin: o, size: size), display: true)
    }
}
