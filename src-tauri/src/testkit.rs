//! Helpers shared by the engine tests and the simulator (test builds only).

use crate::ghost::engine::Sink;
use crate::protocol::CoreCommand;
use std::sync::Mutex;
use tokio::time::Instant;

/// A sink that keeps every command with the time it was sent (tokio's clock, so virtual in paused tests).
#[derive(Default)]
pub struct Recorder(Mutex<Vec<(Instant, CoreCommand)>>);

impl Sink for Recorder {
    fn send(&self, cmd: CoreCommand) {
        self.lock().push((Instant::now(), cmd));
    }
}

impl Recorder {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(Instant, CoreCommand)>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Remove and return the commands sent so far.
    pub fn take(&self) -> Vec<CoreCommand> {
        self.take_timed().into_iter().map(|(_, c)| c).collect()
    }

    /// Remove and return the commands sent so far, with their times.
    pub fn take_timed(&self) -> Vec<(Instant, CoreCommand)> {
        std::mem::take(&mut *self.lock())
    }

    /// The commands sent so far, left in place.
    pub fn peek(&self) -> Vec<(Instant, CoreCommand)> {
        self.lock().clone()
    }

    /// The texts of every `ShowChip` sent so far, left in place.
    pub fn chips(&self) -> Vec<String> {
        self.lock()
            .iter()
            .filter_map(|(_, c)| match c {
                CoreCommand::ShowChip { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}
