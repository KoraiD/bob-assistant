//! What counts as a defect in a simulated session. Each kind is something a user would notice in their text
//! or on screen; see [`super::world`] for where each is checked.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Tab or Shift+Tab inserted something other than the visible chip's next word or rest.
    WrongInsert,
    /// Tab or Shift+Tab inserted text although the chip no longer continued the field.
    AcceptOfStaleChip,
    /// BOB inserted text without a Tab or Shift+Tab.
    UnrequestedInsert,
    /// BOB's insertion created two spaces in a row.
    DoubleSpace,
    /// BOB's insertion put a space before punctuation.
    SpaceBeforePunctuation,
    /// The insertion stopped inside a word that the model's answer continued ("possib").
    Fragment,
    /// A chip kept showing text that no longer continued the field for longer than `STALE_LIMIT`.
    StaleChip,
    /// BOB's insertion ends in a letter right before a letter of the following text ("latestreport").
    GluedWord,
    /// A typo fix deleted characters although the field no longer ended with the misspelled word.
    FixCorruptedText,
    /// A chip with no visible text.
    EmptyChip,
    /// Tab reached the app (a tab character) while a chip was on screen.
    TabLeaked,
    /// The engine showed a chip without capturing keys, or the other way round.
    CaptureMismatch,
    /// Selection Actions or voice edit replaced text without a choice by the user in this episode.
    UnauthorizedReplace,
    /// A replacement aimed at other text than the action ran on.
    ReplaceOfWrongText,
    /// Text was copied to the clipboard without a Copy choice.
    UnrequestedCopy,
    /// Text from a password field, an empty or an oversized selection reached a brain.
    ForbiddenTextSent,
    /// Credential-like text reached a cloud brain.
    SecretToCloud,
    /// Typed, selected or dictated text reached the log or the audit trail.
    TextInLog,
    /// Dictated text was inserted without a transcript to insert, or into a password field.
    UnrequestedDictation,
    /// Recording started in a password field.
    RecordingInSecureField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defect {
    pub kind: Kind,
    pub at_ms: u64,
    pub detail: String,
}
