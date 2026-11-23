import AppKit
import Carbon.HIToolbox
import CoreGraphics
import Testing
@testable import BobNativeCore

@Suite("KeyDecision menu mode (P3, SA-4)")
struct MenuKeyDecisionTests {
    let arrows: CGEventFlags = [.maskNumericPad, .maskSecondaryFn]

    @Test func swallowsExactlyArrowsEnterAndEscapeWhileTheMenuIsOpen() {
        #expect(KeyDecision.decide(keyCode: 126, flags: arrows, capturing: false, menuOpen: true) == .menu(.up))
        #expect(KeyDecision.decide(keyCode: 125, flags: arrows, capturing: false, menuOpen: true) == .menu(.down))
        #expect(KeyDecision.decide(keyCode: 36, flags: [], capturing: false, menuOpen: true) == .menu(.enter))
        #expect(KeyDecision.decide(keyCode: 76, flags: [], capturing: false, menuOpen: true) == .menu(.enter))
        #expect(KeyDecision.decide(keyCode: 53, flags: [], capturing: false, menuOpen: true) == .menu(.escape))
        // Every other key passes, including letters, Tab and shortcuts.
        for code: Int64 in 0..<128 where ![126, 125, 36, 76, 53].contains(code) {
            #expect(KeyDecision.decide(keyCode: code, flags: [], capturing: false, menuOpen: true) == .pass)
        }
        for flags: CGEventFlags in [.maskCommand, .maskControl, .maskAlternate, .maskShift] {
            #expect(KeyDecision.decide(keyCode: 126, flags: flags.union(arrows), capturing: false, menuOpen: true) == .pass)
            #expect(KeyDecision.decide(keyCode: 36, flags: flags, capturing: false, menuOpen: true) == .pass)
        }
    }

    @Test func nothingIsSwallowedWhenTheMenuIsClosedOrTheEventIsBOBs() {
        #expect(KeyDecision.decide(keyCode: 126, flags: arrows, capturing: false) == .pass)
        #expect(KeyDecision.decide(keyCode: 36, flags: [], capturing: true) == .pass)
        #expect(KeyDecision.decide(keyCode: 36, flags: [], capturing: false, synthetic: true, menuOpen: true) == .pass)
    }

    @Test func theMenuTakesPrecedenceOverGhostCapture() {
        #expect(KeyDecision.decide(keyCode: 53, flags: [], capturing: true, menuOpen: true) == .menu(.escape))
        #expect(KeyDecision.decide(keyCode: 48, flags: [], capturing: true, menuOpen: true) == .pass)
    }
}

@Suite("MenuNavigator")
struct MenuNavigatorTests {
    @Test func wrapsAroundAndChoosesTheHighlightedRow() {
        var n = MenuNavigator(count: 3)
        #expect(n.handle(.up) == .moved)
        #expect(n.index == 2)
        _ = n.handle(.down)
        #expect(n.index == 0)
        _ = n.handle(.down)
        #expect(n.handle(.enter) == .chose(1))
        #expect(n.handle(.escape) == .cancelled)
    }

    @Test func anEmptyMenuCanOnlyBeCancelled() {
        var n = MenuNavigator(count: 0)
        #expect(n.handle(.down) == .moved)
        #expect(n.handle(.enter) == .cancelled)
    }

    @Test func clicksSelectOnlyExistingRows() {
        var n = MenuNavigator(count: 2)
        n.select(5)
        #expect(n.index == 0)
        n.select(1)
        #expect(n.index == 1)
    }
}

@Suite("HotkeyCenter")
struct HotkeyTests {
    @Test func mapsModifierNames() {
        #expect(HotkeyCenter.carbonModifiers(["cmd", "shift"]) == UInt32(cmdKey | shiftKey))
        #expect(HotkeyCenter.carbonModifiers(["Control", "option"]) == UInt32(controlKey | optionKey))
        #expect(HotkeyCenter.carbonModifiers(["hyper"]) == nil)
    }

    @Test func plainTypingCanNeverTriggerTheHotkey() {
        #expect(HotkeyCenter.isAcceptable(HotkeyCenter.defaultHotkey))
        #expect(!HotkeyCenter.isAcceptable(Hotkey(keyCode: kVK_ANSI_J, modifiers: [])))
        #expect(!HotkeyCenter.isAcceptable(Hotkey(keyCode: kVK_ANSI_J, modifiers: ["shift"])))
        #expect(!HotkeyCenter.isAcceptable(Hotkey(keyCode: 999, modifiers: ["cmd"])))
    }
}

@Suite("SelectionGuard (P2, SA-7)")
struct SelectionGuardTests {
    @Test func toleratesLineEndingsAndATrimmedTrailingNewline() {
        #expect(SelectionGuard.matches(current: "a\r\nb\n", expected: "a\nb"))
        #expect(SelectionGuard.matches(current: "teh fox", expected: "teh fox"))
    }

    @Test func anyOtherDifferenceBlocksTheReplacement() {
        #expect(!SelectionGuard.matches(current: "teh fox!", expected: "teh fox"))
        #expect(!SelectionGuard.matches(current: "Teh fox", expected: "teh fox"))
        #expect(!SelectionGuard.matches(current: "", expected: ""))
        #expect(!SelectionGuard.matches(current: "x", expected: ""))
    }
}

@Suite("PasteboardSnapshot (P1, SA-2)")
struct PasteboardSnapshotTests {
    @Test func restoreIsTheIdentityOnContents() throws {
        let pb = NSPasteboard(name: NSPasteboard.Name("dev.bob.test.\(UUID().uuidString)"))
        defer { pb.releaseGlobally() }
        pb.clearContents()
        let item = NSPasteboardItem()
        item.setString("hello 👻", forType: .string)
        item.setData(Data([1, 2, 3]), forType: NSPasteboard.PasteboardType("dev.bob.custom"))
        let second = NSPasteboardItem()
        second.setString("second", forType: .string)
        pb.writeObjects([item, second])
        let before = PasteboardSnapshot.capture(pb)
        #expect(before.items.count == 2)

        pb.clearContents()
        pb.setString("borrowed for Cmd+C", forType: .string)
        before.restore(to: pb)
        #expect(PasteboardSnapshot.capture(pb) == before)
        // (`NSPasteboard.string(forType:)` joins all items with newlines, so read the first item itself.)
        #expect(pb.pasteboardItems?.first?.string(forType: .string) == "hello 👻")
    }

    @Test func anEmptyClipboardStaysEmpty() {
        let pb = NSPasteboard(name: NSPasteboard.Name("dev.bob.test.\(UUID().uuidString)"))
        defer { pb.releaseGlobally() }
        pb.clearContents()
        let before = PasteboardSnapshot.capture(pb)
        pb.setString("x", forType: .string)
        before.restore(to: pb)
        #expect(PasteboardSnapshot.capture(pb).items.isEmpty)
    }
}
