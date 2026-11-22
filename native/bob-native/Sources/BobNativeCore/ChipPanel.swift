import AppKit

/// BOB's palette for native overlays. Mirrors the CSS tokens in `src/styles/globals.css`
/// (a test in Task 9 will assert they stay in sync).
public enum Palette {
    public static let ectoplasm = NSColor(srgbRed: 0x8A / 255, green: 0xFF / 255, blue: 0xC1 / 255, alpha: 1)
    public static let midnight = NSColor(srgbRed: 0x0C / 255, green: 0x0B / 255, blue: 0x16 / 255, alpha: 1)
    public static let pumpkin = NSColor(srgbRed: 0xFF / 255, green: 0x9F / 255, blue: 0x5A / 255, alpha: 1)
}

/// The ghost-text chip: a click-through, non-activating panel drawn at the caret.
/// It never takes focus and never receives mouse events (macos-native steering).
public final class ChipPanel: NSPanel {
    private let label = NSTextField(labelWithString: "")

    public init() {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        level = .floating
        ignoresMouseEvents = true
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]

        let content = NSView()
        content.wantsLayer = true
        content.layer?.backgroundColor = Palette.midnight.withAlphaComponent(0.78).cgColor
        content.layer?.cornerRadius = 5
        content.layer?.borderWidth = 0.5
        content.layer?.borderColor = Palette.ectoplasm.withAlphaComponent(0.35).cgColor
        label.textColor = Palette.ectoplasm.withAlphaComponent(0.92)
        label.isBezeled = false
        label.drawsBackground = false
        label.lineBreakMode = .byClipping
        content.addSubview(label)
        contentView = content
        alphaValue = 0
    }

    public override var canBecomeKey: Bool { false }
    public override var canBecomeMain: Bool { false }

    /// Show `text` anchored at `caretAX` (AX coordinates). Hides the chip if the geometry is unusable.
    public func show(text: String, caretAX: Rect) {
        guard !text.isEmpty, CaretRectResolver.isPlausible(caretAX),
              let primary = NSScreen.screens.first else { hide(); return }
        let appKit = CoordinateConverter.appKitRect(fromAX: caretAX, primaryScreenHeight: Double(primary.frame.height))
        let fontSize = min(max(caretAX.height * 0.78, 11), 28)
        label.font = NSFont.systemFont(ofSize: fontSize)
        label.stringValue = text
        label.sizeToFit()
        let padX: CGFloat = 5, padY: CGFloat = 1
        let size = NSSize(width: label.frame.width + padX * 2, height: max(CGFloat(appKit.height), label.frame.height + padY * 2))
        label.setFrameOrigin(NSPoint(x: padX, y: (size.height - label.frame.height) / 2))
        setFrame(NSRect(x: appKit.x + 2, y: appKit.y, width: size.width, height: size.height), display: true)
        orderFrontRegardless()
        if alphaValue < 1 {
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.12
                animator().alphaValue = 1
            }
        }
    }

    public func hide() {
        alphaValue = 0
        orderOut(nil)
    }
}
