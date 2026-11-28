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
    let glyph = Rect(x: 90, y: 200, width: 8, height: 18)

    @Test func nativeTextViewsAgreeAndUseTheGlyphBox() {
        // TextEdit-like: zero-length caret box and previous glyph agree.
        let r = CaretRectResolver.compose(atCaret: Rect(x: 98, y: 200, width: 0, height: 18), previousChar: glyph, nextChar: nil)
        #expect(r?.0 == Rect(x: 98, y: 200, width: 0, height: 18))
        #expect(r?.1 == .previousChar)
    }

    @Test func aTallLineBoxDoesNotInflateTheChip() {
        // iTerm-like: the zero-length query returns a 34 pt box; the glyph is 17 pt and on the same line.
        let lineBox = Rect(x: 98, y: 190, width: 0, height: 34)
        let g = Rect(x: 90, y: 199, width: 8, height: 17)
        #expect(CaretRectResolver.compose(atCaret: lineBox, previousChar: g, nextChar: nil)?.0
                == Rect(x: 98, y: 199, width: 0, height: 17))
    }

    @Test func anEmptyFieldUsesTheLeftEdgeOfAWideBox() {
        // Slack-like: empty field, collapsed range returns the placeholder box (wide); no previous glyph.
        let placeholder = Rect(x: 556, y: 935, width: 658, height: 22)
        #expect(CaretRectResolver.compose(atCaret: placeholder, previousChar: nil, nextChar: nil)?.0
                == Rect(x: 556, y: 935, width: 0, height: 22))
    }

    @Test func aSoftWrappedCaretStaysOnItsOwnLine() {
        // Caret at the start of a wrapped line: the previous glyph ends the line above.
        let caretBox = Rect(x: 20, y: 222, width: 0, height: 18)
        let r = CaretRectResolver.compose(atCaret: caretBox, previousChar: Rect(x: 400, y: 200, width: 6, height: 18), nextChar: nil)
        #expect(r?.0 == caretBox)
        #expect(r?.1 == .atCaret)
    }

    @Test func textEditsFirstLineCaretBoxOneLineTooHighIsIgnored() {
        // Measured in TextEdit (2026-11-24): the glyph before the caret at y 115, the empty range at y 102.
        let r = CaretRectResolver.compose(atCaret: Rect(x: 406.9, y: 102, width: 0, height: 13),
                                          previousChar: Rect(x: 400.3, y: 115, width: 6.6, height: 13), nextChar: nil)
        #expect(r?.1 == .previousChar)
        #expect(r?.0.y == 115)
    }
    @Test func aNewlineBeforeTheCaretIsIgnored() {
        let caretBox = Rect(x: 20, y: 222, width: 0, height: 18)
        let r = CaretRectResolver.compose(atCaret: caretBox, previousChar: Rect(x: 400, y: 200, width: 6, height: 18),
                                          nextChar: nil, previousIsNewline: true)
        #expect(r?.1 == .atCaret)
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

    /// Randomized property: the chip is never taller than the tightest plausible box and always has zero width.
    @Test func composedCaretIsZeroWidthAndNeverTallerThanTheTightestBox() {
        var rng = SeededGenerator(seed: 2026)
        func maybe(_ r: Rect) -> Rect? { Bool.random(using: &rng) ? r : nil }
        for _ in 0..<1000 {
            let y = Double.random(in: 0...1000, using: &rng)
            let at = maybe(Rect(x: Double.random(in: 0...1500, using: &rng), y: y,
                                width: Double.random(in: 0...700, using: &rng), height: Double.random(in: 10...60, using: &rng)))
            let prev = maybe(Rect(x: Double.random(in: 0...1500, using: &rng), y: y + Double.random(in: -30...30, using: &rng),
                                  width: Double.random(in: 1...20, using: &rng), height: Double.random(in: 10...40, using: &rng)))
            guard let (r, _) = CaretRectResolver.compose(atCaret: at, previousChar: prev, nextChar: nil) else {
                #expect(at == nil && prev == nil); continue
            }
            #expect(r.width == 0)
            let tightest = [at, prev].compactMap { $0?.height }.min()!
            #expect(r.height <= max(tightest, [at, prev].compactMap { $0?.height }.max()!))
            if let a = at, prev == nil { #expect(r.x == a.x) }  // left edge of a collapsed box
        }
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

    @Test func keepsAtMostNCharactersFromAStepBoundary() {
        // Ten characters, at most four, steps of four: the window starts at character 8.
        #expect(TextWindow.beforeCaret(value: "0123456789", caretUTF16: 10, maxChars: 4) == "89")
        #expect(TextWindow.beforeCaret(value: "abc", caretUTF16: 3, maxChars: 0) == "")
    }
    /// Code review GT-08: in a long document the window's start stays put while the user types.
    @Test func theWindowStartStaysPutWhileTyping() {
        let doc = String(repeating: "The quarterly report covers sales. ", count: 70)  // 2,450 characters
        var starts = Set<String>()
        for extra in 0..<200 {
            let text = doc + String(repeating: "x", count: extra)
            let window = TextWindow.beforeCaret(value: text, caretUTF16: text.utf16.count)
            #expect(window.count <= TextWindow.defaultMaxChars)
            #expect(text.hasSuffix(window))
            starts.insert(String(window.prefix(40)))
        }
        #expect(starts.count <= 2, "the start moved \(starts.count) times in 200 keys")
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

@Suite("Permissions help text")
struct PermissionsHelpTests {
    @Test func namesTheHostAppForCommonTerminals() {
        #expect(Permissions.hostAppName(termProgram: "iTerm.app") == "iTerm")
        #expect(Permissions.hostAppName(termProgram: "Apple_Terminal") == "Terminal")
        #expect(Permissions.hostAppName(termProgram: "kiro") == "Kiro")
        #expect(Permissions.hostAppName(termProgram: nil).contains("hosts this terminal"))
    }

    @Test func helpTellsTheUserToGrantTheHostNotTheTool() {
        let text = Permissions.accessibilityHelp(termProgram: "iTerm.app")
        #expect(text.contains("iTerm"))
        #expect(text.contains("not to bob-native"))
        #expect(text.contains("Quit iTerm completely"))
    }
}

@Suite("CaretOrientation")
struct CaretOrientationTests {
    let screenH = 1000.0
    let field = Rect(x: 100, y: 200, width: 400, height: 60)  // AX coordinates

    @Test func keepsARectThatAlreadyLiesInsideTheField() {
        let caret = Rect(x: 150, y: 210, width: 0, height: 18)
        #expect(CaretOrientation.classify(caret, elementFrame: field, primaryScreenHeight: screenH) == .asIs(caret))
    }

    @Test func flipsARectReportedInAppKitCoordinates() {
        let truth = Rect(x: 150, y: 210, width: 0, height: 18)
        let appKit = CoordinateConverter.appKitRect(fromAX: truth, primaryScreenHeight: screenH)  // y = 772
        #expect(CaretOrientation.classify(appKit, elementFrame: field, primaryScreenHeight: screenH) == .flipped(truth))
    }

    @Test func rejectsARectOutsideTheFieldInBothOrientations() {
        // The Electron bug report (electron/electron#34755): origin (0, screen height) for every query.
        let bogus = Rect(x: 0, y: 982, width: 0, height: 18)
        #expect(CaretOrientation.classify(bogus, elementFrame: field, primaryScreenHeight: screenH) == .rejected)
    }

    @Test func reportsAmbiguityForTallFieldsAroundTheMiddleOfTheScreen() {
        let tall = Rect(x: 0, y: 100, width: 800, height: 800)
        let caret = Rect(x: 10, y: 300, width: 0, height: 18)
        guard case .ambiguous = CaretOrientation.classify(caret, elementFrame: tall, primaryScreenHeight: screenH) else {
            Issue.record("expected ambiguous"); return
        }
    }

    @Test func trustsAPlausibleRectWhenTheFrameIsUnknown() {
        let caret = Rect(x: 5, y: 5, width: 0, height: 18)
        #expect(CaretOrientation.classify(caret, elementFrame: nil, primaryScreenHeight: screenH) == .asIs(caret))
        #expect(CaretOrientation.classify(Rect(x: 0, y: 0, width: 0, height: 0), elementFrame: nil,
                                          primaryScreenHeight: screenH) == .rejected)
    }

    /// Randomized property: a caret inside the field is always returned unchanged when given as-is,
    /// and a flipped one is either corrected or reported as ambiguous (never silently rejected).
    @Test func caretsInsideTheFieldAreNeverLost() {
        var rng = SeededGenerator(seed: 42)
        for _ in 0..<1000 {
            let h = Double.random(in: 700...2000, using: &rng)
            let f = Rect(x: Double.random(in: 0...1000, using: &rng), y: Double.random(in: 0...(h - 300), using: &rng),
                         width: Double.random(in: 50...600, using: &rng), height: Double.random(in: 30...250, using: &rng))
            let ch = Double.random(in: 10...min(28, f.height), using: &rng)
            let truth = Rect(x: f.x + Double.random(in: 0...f.width, using: &rng),
                             y: f.y + Double.random(in: 0...(f.height - ch), using: &rng), width: 0, height: ch)
            switch CaretOrientation.classify(truth, elementFrame: f, primaryScreenHeight: h) {
            case let .asIs(r): #expect(r == truth)
            case let .ambiguous(asIs, _): #expect(asIs == truth)
            default: Issue.record("lost a caret given as-is")
            }
            let appKit = CoordinateConverter.appKitRect(fromAX: truth, primaryScreenHeight: h)
            switch CaretOrientation.classify(appKit, elementFrame: f, primaryScreenHeight: h) {
            case let .flipped(r): #expect(abs(r.y - truth.y) < 1e-9)
            case let .ambiguous(_, flipped): #expect(abs(flipped.y - truth.y) < 1e-9)
            case .asIs(let r): #expect(abs(r.y - truth.y) < 1e-9)  // only when the flip is the identity
            case .rejected: Issue.record("lost a flipped caret")
            }
        }
    }
}

@Suite("OrientationMemory")
struct OrientationMemoryTests {
    let a = Rect(x: 1, y: 10, width: 0, height: 18)
    let b = Rect(x: 1, y: 900, width: 0, height: 18)

    @Test func defaultsToTheAXConventionWithoutEvidence() {
        var m = OrientationMemory()
        #expect(m.resolve(.ambiguous(asIs: a, flipped: b), app: "x", source: "text_marker") == a)
    }

    @Test func learnsFlippingPerAppAndSource() {
        var m = OrientationMemory()
        _ = m.resolve(.flipped(b), app: "chrome", source: "text_marker")
        #expect(m.resolve(.ambiguous(asIs: a, flipped: b), app: "chrome", source: "text_marker") == b)
        // Other apps and other sources are unaffected.
        #expect(m.resolve(.ambiguous(asIs: a, flipped: b), app: "notes", source: "text_marker") == a)
        #expect(m.resolve(.ambiguous(asIs: a, flipped: b), app: "chrome", source: "range_at_caret") == a)
        // New unambiguous evidence overrides old evidence.
        _ = m.resolve(.asIs(a), app: "chrome", source: "text_marker")
        #expect(m.resolve(.ambiguous(asIs: a, flipped: b), app: "chrome", source: "text_marker") == a)
    }

    @Test func rejectedCandidatesYieldNothing() {
        var m = OrientationMemory()
        #expect(m.resolve(.rejected, app: "x", source: "y") == nil)
    }
}

@Suite("TransientApps")
struct TransientAppsTests {
    @Test func screenshotToolsDoNotCountAsAppSwitches() {
        #expect(TransientApps.ignores("com.apple.screencaptureui"))
        #expect(TransientApps.ignores("com.apple.screenshot.launcher"))
    }

    @Test func realAppsStillCount() {
        for id in ["com.apple.Notes", "com.tinyspeck.slackmacgap", "com.apple.QuickTimePlayerX", "", "com.apple.screencaptureuiX"] {
            #expect(!TransientApps.ignores(id))
        }
        #expect(!TransientApps.ignores(nil))
    }
}
