//! BOB's live state for the orb in the window and the tray (haunted-ui UI-3): idle, listening, thinking,
//! haunting (a suggestion is visible) or error.
//!
//! It is derived from the commands the engines already send to the helper, so no engine needs to know
//! about the UI: [`ObservedSink`] wraps the bridge and feeds every command to [`Activity`]. No user text is
//! kept: only which state is active and, for errors, the short message BOB itself wrote.

use crate::ghost::engine::Sink;
use crate::protocol::{CoreCommand, OrbState as NativeOrb};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long an error stays on the orb.
pub const ERROR_FOR: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Orb {
    Idle,
    Listening,
    Thinking,
    Haunting,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrbView {
    pub state: Orb,
    /// Short, BOB-written explanation for errors.
    pub message: Option<String>,
}

#[derive(Debug, Default)]
struct Inner {
    haunting: bool,
    listening: bool,
    dictation_thinking: bool,
    selection_thinking: bool,
    error: Option<(String, Instant)>,
}

impl Inner {
    /// Priority: error > listening > thinking > haunting > idle.
    fn view(&self, now: Instant) -> OrbView {
        if let Some((m, at)) = &self.error {
            if now.duration_since(*at) < ERROR_FOR {
                return OrbView {
                    state: Orb::Error,
                    message: Some(m.clone()),
                };
            }
        }
        let state = if self.listening {
            Orb::Listening
        } else if self.dictation_thinking || self.selection_thinking {
            Orb::Thinking
        } else if self.haunting {
            Orb::Haunting
        } else {
            Orb::Idle
        };
        OrbView {
            state,
            message: None,
        }
    }
}

type Notify = Box<dyn Fn(&OrbView) + Send + Sync>;

pub struct Activity {
    inner: Mutex<Inner>,
    last: Mutex<OrbView>,
    notify: Notify,
}

impl Activity {
    pub fn new(notify: impl Fn(&OrbView) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            last: Mutex::new(OrbView {
                state: Orb::Idle,
                message: None,
            }),
            notify: Box::new(notify),
        })
    }

    pub fn view(&self) -> OrbView {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .view(Instant::now())
    }

    /// Notify only when the visible state changes.
    fn publish(self: &Arc<Self>) {
        let v = self.view();
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *last != v {
            *last = v.clone();
            drop(last);
            (self.notify)(&v);
        }
    }

    /// Update from one command on its way to the helper.
    pub fn observe(self: &Arc<Self>, cmd: &CoreCommand) {
        let mut error = false;
        {
            let mut i = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match cmd {
                CoreCommand::ShowChip { .. } => i.haunting = true,
                CoreCommand::HideChip => i.haunting = false,
                CoreCommand::Orb { state, text } => {
                    i.listening = *state == NativeOrb::Listening;
                    i.dictation_thinking = *state == NativeOrb::Thinking;
                    if *state == NativeOrb::Error {
                        i.error = Some((text.clone().unwrap_or_default(), Instant::now()));
                        error = true;
                    }
                }
                CoreCommand::MenuStatus {
                    busy,
                    error: e,
                    text,
                } => {
                    i.selection_thinking = *busy;
                    if *e {
                        i.error = Some((text.clone(), Instant::now()));
                        error = true;
                    }
                }
                CoreCommand::HideMenu => i.selection_thinking = false,
                _ => return,
            }
        }
        self.publish();
        if error {
            // Re-publish when the error has expired.
            let me = self.clone();
            std::thread::spawn(move || {
                std::thread::sleep(ERROR_FOR + Duration::from_millis(50));
                me.publish();
            });
        }
    }
}

/// A sink that tells [`Activity`] about every command before forwarding it.
pub struct ObservedSink {
    pub inner: Arc<dyn Sink>,
    pub activity: Arc<Activity>,
}

impl Sink for ObservedSink {
    fn send(&self, cmd: CoreCommand) {
        self.activity.observe(&cmd);
        self.inner.send(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Rect;

    fn rec() -> (Arc<Activity>, Arc<Mutex<Vec<Orb>>>) {
        let seen = Arc::new(Mutex::new(vec![]));
        let s = seen.clone();
        (
            Activity::new(move |v| s.lock().unwrap().push(v.state)),
            seen,
        )
    }

    #[test]
    fn states_follow_commands_with_priority() {
        let (a, seen) = rec();
        a.observe(&CoreCommand::ShowChip {
            text: " hi".into(),
            anchor: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 10.0,
            },
        });
        assert_eq!(a.view().state, Orb::Haunting);
        a.observe(&CoreCommand::Orb {
            state: NativeOrb::Listening,
            text: None,
        });
        assert_eq!(a.view().state, Orb::Listening, "listening beats haunting");
        a.observe(&CoreCommand::Orb {
            state: NativeOrb::Thinking,
            text: None,
        });
        assert_eq!(a.view().state, Orb::Thinking);
        a.observe(&CoreCommand::Orb {
            state: NativeOrb::Done,
            text: None,
        });
        a.observe(&CoreCommand::HideChip);
        assert_eq!(a.view().state, Orb::Idle);
        a.observe(&CoreCommand::MenuStatus {
            text: "Fixing…".into(),
            busy: true,
            error: false,
        });
        assert_eq!(a.view().state, Orb::Thinking);
        a.observe(&CoreCommand::HideMenu);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                Orb::Haunting,
                Orb::Listening,
                Orb::Thinking,
                Orb::Haunting,
                Orb::Idle,
                Orb::Thinking,
                Orb::Idle
            ]
        );
    }

    #[test]
    fn errors_carry_bobs_message_and_expire() {
        let (a, _) = rec();
        a.observe(&CoreCommand::Orb {
            state: NativeOrb::Error,
            text: Some("I didn't hear anything.".into()),
        });
        let v = a.view();
        assert_eq!(v.state, Orb::Error);
        assert_eq!(v.message.as_deref(), Some("I didn't hear anything."));
        let i = a.inner.lock().unwrap();
        assert_eq!(
            i.view(Instant::now() + ERROR_FOR).state,
            Orb::Idle,
            "errors expire"
        );
    }

    #[test]
    fn unrelated_commands_change_nothing_and_are_forwarded() {
        #[derive(Default)]
        struct Count(Mutex<usize>);
        impl Sink for Count {
            fn send(&self, _: CoreCommand) {
                *self.0.lock().unwrap() += 1;
            }
        }
        let (a, seen) = rec();
        let inner = Arc::new(Count::default());
        let s = ObservedSink {
            inner: inner.clone(),
            activity: a,
        };
        s.send(CoreCommand::InsertText { text: "x".into() });
        s.send(CoreCommand::CaptureKeys { active: true });
        assert_eq!(*inner.0.lock().unwrap(), 2);
        assert!(seen.lock().unwrap().is_empty());
    }
}
