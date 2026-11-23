//! Ghost Text (spec `.kiro/specs/ghost-text`): inline next-words prediction at the caret in any app.
//!
//! Pure core: [`redact`], [`prompt`], [`chip`]. The async [`engine`] connects native text events, the
//! Ghost brain and the chip overlay.

pub mod chip;
pub mod engine;
pub mod prompt;
pub mod redact;
