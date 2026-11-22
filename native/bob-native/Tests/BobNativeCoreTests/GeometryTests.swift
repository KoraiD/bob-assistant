import Testing
@testable import BobNativeCore

@Suite("CoordinateConverter")
struct CoordinateConverterTests {
    @Test func flipsTheYAxisAroundThePrimaryScreen() {
        // Primary screen 1000 high. An AX rect at the very top (y: 0, h: 20) sits at AppKit y 980.
        let ax = Rect(x: 10, y: 0, width: 2, height: 20)
        let appKit = CoordinateConverter.appKitRect(fromAX: ax, primaryScreenHeight: 1000)
        #expect(appKit == Rect(x: 10, y: 980, width: 2, height: 20))
    }

    @Test func conversionIsItsOwnInverse() {
        var rng = SeededGenerator(seed: 7)
        for _ in 0..<500 {
            let r = Rect(x: Double.random(in: -3000...3000, using: &rng), y: Double.random(in: -3000...3000, using: &rng),
                         width: Double.random(in: 0...500, using: &rng), height: Double.random(in: 1...100, using: &rng))
            let h = Double.random(in: 600...2200, using: &rng)
            let back = CoordinateConverter.axRect(fromAppKit: CoordinateConverter.appKitRect(fromAX: r, primaryScreenHeight: h),
                                                  primaryScreenHeight: h)
            #expect(abs(back.y - r.y) < 1e-9 && back.x == r.x && back.width == r.width && back.height == r.height)
        }
    }

    @Test func screensAboveThePrimaryHaveNegativeAXYAndStayConsistent() {
        // A display above the primary has negative AX y; the flip still round-trips.
        let ax = Rect(x: 0, y: -300, width: 1, height: 20)
        let appKit = CoordinateConverter.appKitRect(fromAX: ax, primaryScreenHeight: 900)
        #expect(appKit.y == 1180)
    }
}

@Suite("CaretRectResolver")
struct CaretRectResolverTests {
    let good = Rect(x: 100, y: 200, width: 0, height: 18)

    @Test func prefersTheZeroLengthRangeAtTheCaret() {
        let r = CaretRectResolver.resolve(atCaret: good, previousChar: Rect(x: 1, y: 2, width: 3, height: 4), nextChar: nil)
        #expect(r == Rect(x: 100, y: 200, width: 0, height: 18))
    }

    @Test func fallsBackToTheRightEdgeOfThePreviousCharacter() {
        let prev = Rect(x: 90, y: 200, width: 8, height: 18)
        #expect(CaretRectResolver.resolve(atCaret: Rect(x: 0, y: 0, width: 0, height: 0), previousChar: prev, nextChar: nil)
                == Rect(x: 98, y: 200, width: 0, height: 18))
    }

    @Test func fallsBackToTheLeftEdgeOfTheNextCharacter() {
        let next = Rect(x: 120, y: 50, width: 7, height: 16)
        #expect(CaretRectResolver.resolve(atCaret: nil, previousChar: nil, nextChar: next)
                == Rect(x: 120, y: 50, width: 0, height: 16))
    }

    @Test func returnsNilWhenNothingIsPlausible_soTheChipIsHidden() {
        let junk = [Rect(x: 0, y: 0, width: 0, height: 0),
                    Rect(x: .nan, y: 1, width: 1, height: 10),
                    Rect(x: 1, y: 1, width: 1, height: 0),
                    Rect(x: 1, y: 1, width: 1, height: 5000),
                    Rect(x: 1, y: 1, width: .infinity, height: 10)]
        for j in junk { #expect(!CaretRectResolver.isPlausible(j)) }
        #expect(CaretRectResolver.resolve(atCaret: junk[0], previousChar: junk[1], nextChar: junk[2]) == nil)
        #expect(CaretRectResolver.resolve(atCaret: nil, previousChar: nil, nextChar: nil) == nil)
    }
}

@Suite("TextWindow")
struct TextWindowTests {
    @Test func returnsTheTextBeforeTheCaret() {
        #expect(TextWindow.beforeCaret(value: "Hello world", caretUTF16: 5) == "Hello")
    }

    @Test func clampsOutOfRangeOffsets() {
        #expect(TextWindow.beforeCaret(value: "abc", caretUTF16: 99) == "abc")
        #expect(TextWindow.beforeCaret(value: "abc", caretUTF16: -4) == "")
        #expect(TextWindow.beforeCaret(value: "", caretUTF16: 0) == "")
    }

    @Test func keepsOnlyTheLastNCharacters() {
        #expect(TextWindow.beforeCaret(value: "0123456789", caretUTF16: 10, maxChars: 4) == "6789")
        #expect(TextWindow.beforeCaret(value: "abc", caretUTF16: 3, maxChars: 0) == "")
    }

    @Test func utf16OffsetsAreHonoredForEmojiAndNeverCrash() {
        let s = "a😀b"  // 'a'=1 unit, emoji=2 units, 'b'=1 unit
        #expect(TextWindow.beforeCaret(value: s, caretUTF16: 3) == "a😀")
        // A caret inside the surrogate pair yields a replacement character instead of crashing.
        let mid = TextWindow.beforeCaret(value: s, caretUTF16: 2)
        #expect(mid.hasPrefix("a"))
    }

    @Test func neverSplitsAGraphemeCluster() {
        let family = "👨‍👩‍👧"  // one Character, many scalars
        let out = TextWindow.beforeCaret(value: "x" + family, caretUTF16: ("x" + family).utf16.count, maxChars: 1)
        #expect(out == family)
    }

    @Test func resultNeverExceedsTheLimitForRandomInput() {
        var rng = SeededGenerator(seed: 99)
        let alphabet = Array("abc xyz\n😀é漢字")
        for _ in 0..<300 {
            let s = String((0..<Int.random(in: 0...120, using: &rng)).map { _ in alphabet.randomElement(using: &rng)! })
            let caret = Int.random(in: -5...(s.utf16.count + 5), using: &rng)
            let limit = Int.random(in: 0...20, using: &rng)
            #expect(TextWindow.beforeCaret(value: s, caretUTF16: caret, maxChars: limit).count <= limit)
        }
    }
}
