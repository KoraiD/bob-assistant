import AppKit

/// Menu navigation (pure, unit-tested): wraps around, Enter picks, Esc cancels (SA-4).
public struct MenuNavigator: Equatable, Sendable {
    public enum Outcome: Equatable, Sendable { case moved, chose(Int), cancelled }

    public private(set) var count: Int
    public private(set) var index: Int

    public init(count: Int) {
        self.count = max(count, 0)
        index = 0
    }

    public mutating func handle(_ key: MenuKey) -> Outcome {
        switch key {
        case .up:
            if count > 0 { index = (index - 1 + count) % count }
            return .moved
        case .down:
            if count > 0 { index = (index + 1) % count }
            return .moved
        case .enter:
            return count > 0 ? .chose(index) : .cancelled
        case .escape:
            return .cancelled
        }
    }

    public mutating func select(_ i: Int) {
        if (0..<count).contains(i) { index = i }
    }
}

/// The Selection Actions menu: a non-activating floating panel near the selection. It never becomes key,
/// so the user's app keeps focus and its selection; keys arrive through the event tap (SA-4). Rows are
/// clickable. After a choice the panel turns into a status line (progress, preview, error).
public final class ActionMenuPanel: NSPanel {
    static let width: CGFloat = 340
    static let rowHeight: CGFloat = 26

    private let stack = NSStackView()
    private var rows: [NSTextField] = []
    private(set) public var navigator = MenuNavigator(count: 0)
    public private(set) var isMenuVisible = false
    /// Called with the chosen row (click).
    public var onClick: ((Int) -> Void)?

    public init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: Self.width, height: 80),
                   styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        level = .popUpMenu
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]

        let content = NSView()
        content.wantsLayer = true
        content.layer?.backgroundColor = Palette.midnight.withAlphaComponent(0.94).cgColor
        content.layer?.cornerRadius = 10
        content.layer?.borderWidth = 1
        content.layer?.borderColor = Palette.ectoplasm.withAlphaComponent(0.35).cgColor
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 2
        stack.edgeInsets = NSEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            stack.topAnchor.constraint(equalTo: content.topAnchor),
            stack.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        ])
        contentView = content
    }

    public override var canBecomeKey: Bool { false }
    public override var canBecomeMain: Bool { false }

    /// `centered`: a one-line row taller than its text, drawn vertically centered (menu rows).
    private func label(_ text: String, size: CGFloat = 13, color: NSColor = .white, wraps: Bool = false,
                       lines: Int = 8, centered: Bool = false) -> NSTextField {
        let l = wraps ? NSTextField(wrappingLabelWithString: text) : NSTextField(labelWithString: text)
        if centered {
            let cell = VerticallyCenteredCell(textCell: text)
            cell.isEditable = false
            cell.isSelectable = false
            cell.isBordered = false
            cell.drawsBackground = false
            l.cell = cell
        }
        l.font = .systemFont(ofSize: size)
        l.textColor = color
        l.lineBreakMode = wraps ? .byWordWrapping : .byTruncatingTail
        l.maximumNumberOfLines = wraps ? lines : 1
        l.preferredMaxLayoutWidth = Self.width - 32
        l.wantsLayer = true
        l.layer?.cornerRadius = 6
        l.translatesAutoresizingMaskIntoConstraints = false
        l.widthAnchor.constraint(equalToConstant: Self.width - 16).isActive = true
        return l
    }

    private func clear() {
        stack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        rows = []
    }

    /// Show the actions near `anchorAX` (selection bounds, AX coordinates), or near the mouse. `preview` is
    /// a result shown above the items (preview before replace); without an anchor an open panel stays put.
    public func showMenu(title: String, items: [String], anchorAX: Rect?, preview: String? = nil) {
        let keepPlace = anchorAX == nil && isVisible
        clear()
        stack.addArrangedSubview(label("👻 \(title)", size: 11, color: Palette.ectoplasm.withAlphaComponent(0.85)))
        if let preview {
            let p = label(preview, size: 13, color: NSColor.white.withAlphaComponent(0.95), wraps: true, lines: 14)
            p.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.06).cgColor
            stack.addArrangedSubview(p)
        }
        for (i, item) in items.enumerated() {
            let row = label("  \(item)", centered: true)
            row.heightAnchor.constraint(equalToConstant: Self.rowHeight).isActive = true
            row.addGestureRecognizer(NSClickGestureRecognizer(target: self, action: #selector(rowClicked(_:))))
            row.tag = i
            rows.append(row)
            stack.addArrangedSubview(row)
        }
        stack.addArrangedSubview(label("↑↓ choose · ⏎ apply · esc close", size: 10,
                                       color: NSColor.white.withAlphaComponent(0.5)))
        navigator = MenuNavigator(count: items.count)
        highlight()
        if keepPlace { resizeToFit() } else { place(near: anchorAX) }
        isMenuVisible = true
        orderFrontRegardless()
    }

    /// Replace the rows with a status line (wrapping), e.g. "Fixing grammar…" or a streamed preview.
    public func showStatus(_ text: String, busy: Bool, error: Bool) {
        // Voice edits show a status without a menu first: place the panel before showing it.
        let needsPlacement = !isVisible
        isMenuVisible = false
        clear()
        let color = error ? Palette.pumpkin : NSColor.white.withAlphaComponent(busy ? 0.75 : 0.95)
        stack.addArrangedSubview(label("👻 BOB", size: 11, color: Palette.ectoplasm.withAlphaComponent(0.85)))
        stack.addArrangedSubview(label(text, color: color, wraps: true))
        if needsPlacement { place(near: nil) }
        resizeToFit()
        orderFrontRegardless()
    }

    public func hideMenu() {
        isMenuVisible = false
        orderOut(nil)
    }

    public func move(_ key: MenuKey) -> MenuNavigator.Outcome {
        let outcome = navigator.handle(key)
        highlight()
        return outcome
    }

    @objc private func rowClicked(_ sender: NSClickGestureRecognizer) {
        guard let i = sender.view?.tag else { return }
        navigator.select(i)
        highlight()
        onClick?(i)
    }

    private func highlight() {
        for (i, row) in rows.enumerated() {
            let on = i == navigator.index
            row.layer?.backgroundColor = on ? Palette.ectoplasm.withAlphaComponent(0.22).cgColor : NSColor.clear.cgColor
            row.textColor = on ? Palette.ectoplasm : .white
        }
    }

    private func resizeToFit() {
        stack.layoutSubtreeIfNeeded()
        let h = stack.fittingSize.height
        var f = frame
        f.origin.y += f.height - h  // keep the top edge in place
        f.size = NSSize(width: Self.width, height: h)
        setFrame(f, display: true)
    }

    private func place(near anchorAX: Rect?) {
        stack.layoutSubtreeIfNeeded()
        let size = NSSize(width: Self.width, height: stack.fittingSize.height)
        let primaryHeight = Double(NSScreen.screens.first?.frame.height ?? 0)
        var origin: NSPoint
        if let a = anchorAX, a.x.isFinite, a.y.isFinite {
            let k = CoordinateConverter.appKitRect(fromAX: a, primaryScreenHeight: primaryHeight)
            origin = NSPoint(x: k.x, y: k.y - size.height - 6)  // below the selection
        } else {
            let m = NSEvent.mouseLocation
            origin = NSPoint(x: m.x, y: m.y - size.height - 12)
        }
        // Keep the panel on the screen that contains the anchor.
        let screen = NSScreen.screens.first { $0.frame.contains(origin) } ?? NSScreen.main
        if let vf = screen?.visibleFrame {
            if origin.y < vf.minY { origin.y = min(vf.maxY - size.height, origin.y + size.height + 30) }
            origin.x = min(max(origin.x, vf.minX + 4), vf.maxX - size.width - 4)
            origin.y = min(max(origin.y, vf.minY + 4), vf.maxY - size.height - 4)
        }
        setFrame(NSRect(origin: origin, size: size), display: true)
    }
}

/// A label cell that draws its single line in the vertical middle of a taller row (NSTextField draws at the top).
final class VerticallyCenteredCell: NSTextFieldCell {
    override func drawingRect(forBounds rect: NSRect) -> NSRect {
        let base = super.drawingRect(forBounds: rect)
        let height = cellSize(forBounds: rect).height
        guard height < base.height else { return base }
        return NSRect(x: base.minX, y: base.minY + (base.height - height) / 2, width: base.width, height: height)
    }
}
