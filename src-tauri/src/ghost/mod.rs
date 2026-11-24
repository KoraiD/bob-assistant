//! Ghost Text (spec `.kiro/specs/ghost-text`): inline next-words prediction at the caret in any app.
//!
//! Pure core: [`redact`], [`prompt`], [`chip`]. The async [`engine`] connects native text events, the
//! Ghost brain and the chip overlay.

pub mod chip;
pub mod engine;
#[cfg(test)]
mod matrix_tests;
pub mod memory;
pub mod prompt;
pub mod redact;
pub mod settings;
