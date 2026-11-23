//! Voice Dictation: hold a hotkey, speak, release; the text lands at the caret (spec `voice-dictation`).
//! Audio is captured and transcribed on the device by the Swift helper; it never reaches this process.

pub mod engine;
pub mod machine;
pub mod settings;
