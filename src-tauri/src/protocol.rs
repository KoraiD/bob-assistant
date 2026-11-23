//! Wire protocol between the Rust core and the Swift helper (`bob-native`).
//!
//! Transport framing is newline-delimited JSON (one object per line, UTF-8). Every object has a
//! `type` field. Unknown `type` values decode to the `Unknown` variant so a newer peer can never
//! crash an older one (requirement NB-3). Byte chunking never affects decoding (NB-4).
//!
//! The Swift mirror lives in `native/bob-native/Sources/BobNativeCore/Protocol.swift`; both sides are
//! tested against the shared fixtures in `native/bob-native/Tests/Fixtures/`.
//!
//! PRIVACY: events can carry text typed by the user (`TextState::before_caret`). They must never be
//! logged (NB-8). `Debug` for those payloads is redacted below.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Bumped on every incompatible schema change. Both sides refuse to talk on a mismatch (NB-2).
/// v3: personal context (`context_state`, `screen_text`, `set_context_mode`, `request_screen_recording`,
/// `permissions.screen_recording`) and Selection Actions (`selection`, `menu_choice`, `menu_cancel`,
/// `replace_result`, `show_menu`, `menu_status`, `hide_menu`, `replace_selection`, `set_hotkey`) and Voice
/// Dictation (`dictation_key`, `transcript`, `dictation_error`, `dictation_status`, `set_dictation`,
/// `start_recording`, `stop_recording`, `orb`).
pub const PROTOCOL_VERSION: u32 = 3;

/// Longest accepted line in bytes. Longer lines are dropped, not buffered without bound.
pub const MAX_LINE_BYTES: usize = 1 << 20;

/// A rectangle in screen points, **top-left origin** (Accessibility coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Microphone permission as reported by macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicStatus {
    Granted,
    Denied,
    Undetermined,
}

/// Keys the helper captures, and only while the core has asked it to (`CaptureKeys { active: true }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Tab,
    ShiftTab,
    Escape,
}

/// How much of the screen BOB may read for context (personal-context spec, PC-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextScope {
    /// Accessibility only: window title and the focused field's text. No Screen Recording permission.
    #[default]
    Accessibility,
    /// Plus the text visible in the frontmost window (screen capture + on-device OCR).
    ActiveWindow,
    /// Plus the text visible on every display.
    AllScreens,
}

/// Where the helper read the selection from (SA-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    /// `AXSelectedText` of the focused element.
    Accessibility,
    /// Cmd+C with the clipboard saved and restored.
    Copy,
}

/// A global hotkey: macOS virtual key code plus modifiers ("cmd", "shift", "option", "control").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub key_code: u32,
    pub modifiers: Vec<String>,
}

/// What the dictation orb shows (VD-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbState {
    Hidden,
    Listening,
    Thinking,
    Done,
    Error,
}

/// Messages sent by the Swift helper to the core.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeEvent {
    Hello {
        version: u32,
        pid: u32,
    },
    Permissions {
        accessibility: bool,
        microphone: MicStatus,
        #[serde(default)]
        screen_recording: bool,
    },
    Focus {
        app_bundle_id: String,
        app_name: String,
        secure: bool,
        editable: bool,
    },
    TextState {
        app_bundle_id: String,
        secure: bool,
        before_caret: String,
        selection_len: u32,
        caret: Option<Rect>,
    },
    /// Surroundings of the focused field, sent when they change (PC-3): the focused window's title and
    /// the text after the caret (bounded). Never sent for secure fields.
    ContextState {
        app_bundle_id: String,
        window_title: Option<String>,
        after_caret: String,
    },
    /// Text recognized on screen (PC-4), only when the core enabled a capture scope and Screen
    /// Recording is granted. `ms` is how long capture + OCR took.
    ScreenText {
        scope: ContextScope,
        app_bundle_id: String,
        text: String,
        ms: u32,
    },
    /// The Selection Actions hotkey was pressed (SA-1). `text` is empty when nothing is selected or the
    /// field is secure (never read, SA-10). `anchor` is the selection's bounds when known.
    Selection {
        app_bundle_id: String,
        text: String,
        secure: bool,
        source: Option<SelectionSource>,
        anchor: Option<Rect>,
    },
    /// The user picked menu item `index` (Enter or click).
    MenuChoice {
        index: u32,
    },
    /// The menu was dismissed (Esc, app switch, click elsewhere).
    MenuCancel,
    /// Outcome of `replace_selection` (SA-6, SA-7). `method` is "accessibility" or "paste".
    ReplaceResult {
        ok: bool,
        method: Option<String>,
        error: Option<String>,
    },
    /// The dictation hotkey went down or up (VD-1). The core's state machine decides what happens.
    DictationKey {
        pressed: bool,
    },
    /// Result of a recording (VD-2, VD-5). `speech` is false when the speech gate found no speech, in
    /// which case `text` is empty and nothing was transcribed. Audio never leaves the helper (VD-8).
    Transcript {
        text: String,
        language: Option<String>,
        speech: bool,
        audio_ms: u32,
        transcribe_ms: u32,
    },
    /// Recording or transcription failed; `reason` is "microphone", "no_model" or a short message.
    DictationError {
        reason: String,
    },
    /// The dictation model finished loading (or failed to).
    DictationStatus {
        model_loaded: bool,
        error: Option<String>,
    },
    Error {
        message: String,
    },
    /// A captured key press (GT-10 to GT-12). Never sent while capture is off (GT-13).
    Key {
        key: Key,
    },
    /// Any `type` this build does not know.
    #[serde(other)]
    Unknown,
}

/// Messages sent by the core to the Swift helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreCommand {
    HelloAck {
        version: u32,
    },
    Reject {
        reason: String,
    },
    ShowChip {
        text: String,
        anchor: Rect,
    },
    HideChip,
    /// Type `text` at the caret of the focused app (GT-14).
    InsertText {
        text: String,
    },
    /// Swallow Tab / Shift+Tab / Esc and report them as `Key` events, only while `active` (GT-13).
    CaptureKeys {
        active: bool,
    },
    RequestPermissions,
    /// Which context the helper may gather. Apps in `exclude` (quiet apps) are never captured.
    SetContextMode {
        scope: ContextScope,
        exclude: Vec<String>,
    },
    /// Show macOS's Screen Recording prompt (only when the user picks a capture scope).
    RequestScreenRecording,
    /// Show the Selection Actions menu near `anchor` (SA-1). The helper owns navigation (SA-4).
    ShowMenu {
        title: String,
        items: Vec<String>,
        anchor: Option<Rect>,
    },
    /// Replace the menu with a status line: progress, a streamed preview, or an error (SA-5, SA-7).
    MenuStatus {
        text: String,
        busy: bool,
        error: bool,
    },
    HideMenu,
    /// Replace the selection with `text`, only if it still equals `expected` (SA-6, SA-7).
    ReplaceSelection {
        expected: String,
        text: String,
    },
    /// Register the Selection Actions hotkey (replaces the previous one).
    SetHotkey {
        hotkey: Hotkey,
    },
    /// Configure Voice Dictation: the hold-to-talk hotkey (None turns dictation off), the whisper model to
    /// keep loaded, and the language ("" = detect).
    SetDictation {
        hotkey: Option<Hotkey>,
        model_path: Option<String>,
        language: String,
    },
    StartRecording,
    /// Stop recording; transcribe only when `transcribe` (a too-short hold is discarded, VD-4).
    StopRecording {
        transcribe: bool,
    },
    /// Show the dictation orb in a state, with an optional short message.
    Orb {
        state: OrbState,
        text: Option<String>,
    },
    Quit,
    /// Any `type` this build does not know.
    #[serde(other)]
    Unknown,
}

// Manual Debug so typed text can never leak through `{:?}` logging.
impl fmt::Debug for NativeEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeEvent::Hello { version, pid } => f
                .debug_struct("Hello")
                .field("version", version)
                .field("pid", pid)
                .finish(),
            NativeEvent::Permissions {
                accessibility,
                microphone,
                screen_recording,
            } => f
                .debug_struct("Permissions")
                .field("accessibility", accessibility)
                .field("microphone", microphone)
                .field("screen_recording", screen_recording)
                .finish(),
            NativeEvent::ContextState {
                app_bundle_id,
                window_title,
                after_caret,
            } => f
                .debug_struct("ContextState")
                .field("app_bundle_id", app_bundle_id)
                .field("has_window_title", &window_title.is_some())
                .field("after_caret_chars", &after_caret.chars().count())
                .finish(),
            NativeEvent::ScreenText {
                scope,
                app_bundle_id,
                text,
                ms,
            } => f
                .debug_struct("ScreenText")
                .field("scope", scope)
                .field("app_bundle_id", app_bundle_id)
                .field("text_chars", &text.chars().count())
                .field("ms", ms)
                .finish(),
            NativeEvent::Focus {
                app_bundle_id,
                secure,
                editable,
                ..
            } => f
                .debug_struct("Focus")
                .field("app_bundle_id", app_bundle_id)
                .field("secure", secure)
                .field("editable", editable)
                .finish(),
            NativeEvent::TextState {
                app_bundle_id,
                secure,
                before_caret,
                selection_len,
                caret,
            } => f
                .debug_struct("TextState")
                .field("app_bundle_id", app_bundle_id)
                .field("secure", secure)
                .field("before_caret_chars", &before_caret.chars().count())
                .field("selection_len", selection_len)
                .field("has_caret", &caret.is_some())
                .finish(),
            NativeEvent::Selection {
                app_bundle_id,
                text,
                secure,
                source,
                anchor,
            } => f
                .debug_struct("Selection")
                .field("app_bundle_id", app_bundle_id)
                .field("text_chars", &text.chars().count())
                .field("secure", secure)
                .field("source", source)
                .field("has_anchor", &anchor.is_some())
                .finish(),
            NativeEvent::MenuChoice { index } => {
                f.debug_struct("MenuChoice").field("index", index).finish()
            }
            NativeEvent::MenuCancel => f.write_str("MenuCancel"),
            NativeEvent::ReplaceResult { ok, method, .. } => f
                .debug_struct("ReplaceResult")
                .field("ok", ok)
                .field("method", method)
                .finish_non_exhaustive(),
            NativeEvent::DictationKey { pressed } => f
                .debug_struct("DictationKey")
                .field("pressed", pressed)
                .finish(),
            NativeEvent::Transcript {
                text,
                language,
                speech,
                audio_ms,
                transcribe_ms,
            } => f
                .debug_struct("Transcript")
                .field("text_chars", &text.chars().count())
                .field("language", language)
                .field("speech", speech)
                .field("audio_ms", audio_ms)
                .field("transcribe_ms", transcribe_ms)
                .finish(),
            NativeEvent::DictationError { reason } => f
                .debug_struct("DictationError")
                .field("reason", reason)
                .finish(),
            NativeEvent::DictationStatus {
                model_loaded,
                error,
            } => f
                .debug_struct("DictationStatus")
                .field("model_loaded", model_loaded)
                .field("error", error)
                .finish(),
            NativeEvent::Error { .. } => f.debug_struct("Error").finish_non_exhaustive(),
            NativeEvent::Key { key } => f.debug_struct("Key").field("key", key).finish(),
            NativeEvent::Unknown => f.write_str("Unknown"),
        }
    }
}

/// One decoded frame from the byte stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A complete, non-empty line (without the trailing newline).
    Line(String),
    /// A line exceeded [`MAX_LINE_BYTES`] and was discarded up to the next newline.
    TooLong,
    /// A line was not valid UTF-8 and was discarded.
    InvalidUtf8,
}

/// Incremental newline-delimited frame decoder. Feed it arbitrary byte chunks.
#[derive(Debug, Default)]
pub struct LineDecoder {
    buf: Vec<u8>,
    discarding: bool,
}

impl LineDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume `chunk` and return every frame completed by it, in order.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        let mut out = Vec::new();
        for &b in chunk {
            if b == b'\n' {
                if self.discarding {
                    self.discarding = false;
                    self.buf.clear();
                } else if !self.buf.is_empty() {
                    let bytes = std::mem::take(&mut self.buf);
                    match String::from_utf8(bytes) {
                        Ok(s) => {
                            let s = s.trim_end_matches('\r').to_string();
                            if !s.is_empty() {
                                out.push(Frame::Line(s));
                            }
                        }
                        Err(_) => out.push(Frame::InvalidUtf8),
                    }
                }
            } else if !self.discarding {
                self.buf.push(b);
                if self.buf.len() > MAX_LINE_BYTES {
                    self.buf.clear();
                    self.discarding = true;
                    out.push(Frame::TooLong);
                }
            }
        }
        out
    }
}

/// Encode a message as one JSON line, including the trailing newline.
pub fn encode_line<T: Serialize>(msg: &T) -> String {
    // Serializing these plain data types cannot fail.
    let mut s = serde_json::to_string(msg).expect("protocol messages are always serializable");
    s.push('\n');
    s
}

/// Decode a helper event from one line.
pub fn parse_event(line: &str) -> Result<NativeEvent, serde_json::Error> {
    serde_json::from_str(line)
}

/// Decode a core command from one line.
pub fn parse_command(line: &str) -> Result<CoreCommand, serde_json::Error> {
    serde_json::from_str(line)
}

/// Check a helper's announced protocol version. `Err` carries the reason sent in `Reject`.
pub fn check_version(helper_version: u32) -> Result<(), String> {
    if helper_version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(format!(
            "protocol version mismatch: core {PROTOCOL_VERSION}, helper {helper_version}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const EVENT_FIXTURES: &str =
        include_str!("../../native/bob-native/Tests/Fixtures/native_events.jsonl");
    const COMMAND_FIXTURES: &str =
        include_str!("../../native/bob-native/Tests/Fixtures/core_commands.jsonl");

    fn fixture_lines(s: &str) -> Vec<&str> {
        s.lines().filter(|l| !l.trim().is_empty()).collect()
    }

    // ---- shared fixtures (the Swift tests read the same files) ----

    #[test]
    fn every_event_fixture_decodes_and_reencodes_to_the_same_json() {
        let lines = fixture_lines(EVENT_FIXTURES);
        assert!(lines.len() >= 6);
        for line in lines {
            let ev = parse_event(line).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_ne!(
                ev,
                NativeEvent::Unknown,
                "fixture decoded as Unknown: {line}"
            );
            let back: serde_json::Value =
                serde_json::from_str(encode_line(&ev).trim_end()).unwrap();
            let orig: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(back, orig, "round trip changed {line}");
        }
    }

    #[test]
    fn every_command_fixture_decodes_and_reencodes_to_the_same_json() {
        for line in fixture_lines(COMMAND_FIXTURES) {
            let cmd = parse_command(line).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_ne!(
                cmd,
                CoreCommand::Unknown,
                "fixture decoded as Unknown: {line}"
            );
            let back: serde_json::Value =
                serde_json::from_str(encode_line(&cmd).trim_end()).unwrap();
            let orig: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(back, orig, "round trip changed {line}");
        }
    }

    // ---- unit tests ----

    #[test]
    fn unknown_type_is_tolerated_not_fatal() {
        assert_eq!(
            parse_event(r#"{"type":"from_the_future","x":1}"#).unwrap(),
            NativeEvent::Unknown
        );
        assert_eq!(
            parse_command(r#"{"type":"from_the_future"}"#).unwrap(),
            CoreCommand::Unknown
        );
    }

    #[test]
    fn missing_type_is_an_error_not_a_panic() {
        assert!(parse_event(r#"{"version":1}"#).is_err());
        assert!(parse_event("not json").is_err());
    }

    #[test]
    fn debug_output_never_contains_typed_text() {
        let ev = NativeEvent::TextState {
            app_bundle_id: "com.example".into(),
            secure: false,
            before_caret: "my secret sentence".into(),
            selection_len: 0,
            caret: None,
        };
        let shown = format!("{ev:?}");
        assert!(!shown.contains("secret"), "{shown}");
        assert!(shown.contains("before_caret_chars"));
        let ctx = format!(
            "{:?}",
            NativeEvent::ContextState {
                app_bundle_id: "com.example".into(),
                window_title: Some("Re: secret plans".into()),
                after_caret: "secret tail".into(),
            }
        );
        assert!(!ctx.contains("secret"), "{ctx}");
        let screen = format!(
            "{:?}",
            NativeEvent::ScreenText {
                scope: ContextScope::AllScreens,
                app_bundle_id: "com.example".into(),
                text: "secret screen".into(),
                ms: 120,
            }
        );
        assert!(!screen.contains("secret"), "{screen}");
        let err = format!(
            "{:?}",
            NativeEvent::Error {
                message: "private path".into()
            }
        );
        assert!(!err.contains("private"));
    }

    #[test]
    fn decoder_handles_split_and_joined_chunks() {
        let mut d = LineDecoder::new();
        assert!(d.push(b"{\"type\":\"qu").is_empty());
        assert_eq!(
            d.push(b"it\"}\n{\"type\":\"hide_chip\"}\n"),
            vec![
                Frame::Line("{\"type\":\"quit\"}".into()),
                Frame::Line("{\"type\":\"hide_chip\"}".into()),
            ]
        );
    }

    #[test]
    fn decoder_skips_blank_lines_and_trims_crlf() {
        let mut d = LineDecoder::new();
        assert_eq!(d.push(b"\n\r\nabc\r\n"), vec![Frame::Line("abc".into())]);
    }

    #[test]
    fn decoder_drops_oversized_lines_and_recovers() {
        let mut d = LineDecoder::new();
        let big = vec![b'a'; MAX_LINE_BYTES + 10];
        let mut frames = d.push(&big);
        frames.extend(d.push(b"tail-of-big-line\nok\n"));
        assert_eq!(frames, vec![Frame::TooLong, Frame::Line("ok".into())]);
    }

    #[test]
    fn decoder_reports_invalid_utf8_and_continues() {
        let mut d = LineDecoder::new();
        let frames = d.push(&[0xff, 0xfe, b'\n', b'o', b'k', b'\n']);
        assert_eq!(frames, vec![Frame::InvalidUtf8, Frame::Line("ok".into())]);
    }

    #[test]
    fn version_check_accepts_only_the_current_version() {
        assert!(check_version(PROTOCOL_VERSION).is_ok());
        let reason = check_version(PROTOCOL_VERSION + 1).unwrap_err();
        assert!(reason.contains("mismatch"));
    }

    // ---- property tests (requirements NB-3, NB-4) ----

    fn arb_rect() -> impl Strategy<Value = Rect> {
        (-1e6f64..1e6, -1e6f64..1e6, 0f64..1e4, 0f64..1e4).prop_map(|(x, y, width, height)| Rect {
            x,
            y,
            width,
            height,
        })
    }

    fn arb_event() -> impl Strategy<Value = NativeEvent> {
        prop_oneof![
            (any::<u32>(), any::<u32>())
                .prop_map(|(version, pid)| NativeEvent::Hello { version, pid }),
            (
                any::<bool>(),
                prop_oneof![
                    Just(MicStatus::Granted),
                    Just(MicStatus::Denied),
                    Just(MicStatus::Undetermined)
                ]
            )
                .prop_map(|(accessibility, microphone)| NativeEvent::Permissions {
                    accessibility,
                    microphone,
                    screen_recording: accessibility,
                }),
            (".{0,30}", proptest::option::of(".{0,40}"), "(?s).{0,200}").prop_map(
                |(app_bundle_id, window_title, after_caret)| NativeEvent::ContextState {
                    app_bundle_id,
                    window_title,
                    after_caret
                }
            ),
            (".{0,30}", "(?s).{0,300}", any::<u32>()).prop_map(|(app_bundle_id, text, ms)| {
                NativeEvent::ScreenText {
                    scope: ContextScope::ActiveWindow,
                    app_bundle_id,
                    text,
                    ms,
                }
            }),
            (".{0,30}", ".{0,30}", any::<bool>(), any::<bool>()).prop_map(
                |(app_bundle_id, app_name, secure, editable)| NativeEvent::Focus {
                    app_bundle_id,
                    app_name,
                    secure,
                    editable
                }
            ),
            (
                ".{0,30}",
                any::<bool>(),
                "(?s).{0,200}",
                any::<u32>(),
                proptest::option::of(arb_rect())
            )
                .prop_map(
                    |(app_bundle_id, secure, before_caret, selection_len, caret)| {
                        NativeEvent::TextState {
                            app_bundle_id,
                            secure,
                            before_caret,
                            selection_len,
                            caret,
                        }
                    }
                ),
            "(?s).{0,60}".prop_map(|message| NativeEvent::Error { message }),
            prop_oneof![Just(Key::Tab), Just(Key::ShiftTab), Just(Key::Escape)]
                .prop_map(|key| NativeEvent::Key { key }),
        ]
    }

    /// Split `bytes` at the given fractional cut points (sorted, deduped).
    fn split_at_fractions(bytes: &[u8], cuts: &[f64]) -> Vec<Vec<u8>> {
        let mut idx: Vec<usize> = cuts
            .iter()
            .map(|c| ((bytes.len() as f64) * c) as usize)
            .collect();
        idx.push(0);
        idx.push(bytes.len());
        idx.sort_unstable();
        idx.dedup();
        idx.windows(2).map(|w| bytes[w[0]..w[1]].to_vec()).collect()
    }

    proptest! {
        /// NB-4 / P1: for any messages and any chunking, decoded events equal the originals.
        #[test]
        fn decoding_is_independent_of_chunking(
            events in proptest::collection::vec(arb_event(), 0..8),
            cuts in proptest::collection::vec(0f64..1.0, 0..12),
        ) {
            let stream: String = events.iter().map(encode_line).collect();
            let mut decoder = LineDecoder::new();
            let mut decoded = Vec::new();
            for chunk in split_at_fractions(stream.as_bytes(), &cuts) {
                for frame in decoder.push(&chunk) {
                    match frame {
                        Frame::Line(l) => decoded.push(parse_event(&l).unwrap()),
                        other => prop_assert!(false, "unexpected frame {:?}", other),
                    }
                }
            }
            prop_assert_eq!(decoded, events);
        }

        /// NB-3 / P2: arbitrary bytes never panic the decoder or the parsers.
        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let mut decoder = LineDecoder::new();
            for frame in decoder.push(&bytes) {
                if let Frame::Line(l) = frame {
                    let _ = parse_event(&l);
                    let _ = parse_command(&l);
                }
            }
        }

        /// NB-3: any object with an unrecognized `type` decodes to Unknown.
        #[test]
        fn unknown_types_decode_to_unknown(t in "[a-z_]{1,20}") {
            let known = ["hello","permissions","focus","text_state","error","key",
                         "hello_ack","reject","show_chip","hide_chip","insert_text","capture_keys","request_permissions","quit"];
            prop_assume!(!known.contains(&t.as_str()));
            let line = format!(r#"{{"type":"{t}","payload":[1,2,3]}}"#);
            prop_assert_eq!(parse_event(&line).unwrap(), NativeEvent::Unknown);
            prop_assert_eq!(parse_command(&line).unwrap(), CoreCommand::Unknown);
        }

        /// NB-2 / P3: only the exact current version passes the handshake.
        #[test]
        fn only_current_version_is_accepted(v in any::<u32>()) {
            prop_assert_eq!(check_version(v).is_ok(), v == PROTOCOL_VERSION);
        }
    }
}
