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
    /// A chip with no visible text.
    EmptyChip,
    /// Tab reached the app (a tab character) while a chip was on screen.
    TabLeaked,
    /// The engine showed a chip without capturing keys, or the other way round.
    CaptureMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defect {
    pub kind: Kind,
    pub at_ms: u64,
    pub detail: String,
}
