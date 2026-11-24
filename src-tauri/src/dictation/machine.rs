//! The dictation state machine (spec `voice-dictation`): pure, table-driven, property-tested (P1, P2).
//!
//! `Idle -> Recording -> (Idle: discarded | Transcribing -> (Cleaning ->) Idle: inserted)`.
//! Time is passed in as milliseconds so the machine is deterministic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// VD-4: shorter holds are discarded.
    pub min_hold_ms: u64,
    /// VD-11: recordings stop at this length.
    pub max_ms: u64,
    /// Proposal 3: a quick tap latches recording on; the next press stops it.
    pub tap_to_toggle: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_hold_ms: 250,
            max_ms: 300_000,
            tap_to_toggle: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording {
        started_ms: u64,
    },
    /// Hands-free recording after a quick tap (tap-to-toggle); the next press stops it.
    Latched {
        started_ms: u64,
    },
    Transcribing,
    /// Waiting for the brain to clean up `raw` (VD-6).
    Cleaning {
        raw: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Press {
        at_ms: u64,
    },
    Release {
        at_ms: u64,
    },
    /// Periodic clock while recording (enforces the cap).
    Tick {
        at_ms: u64,
    },
    Transcript {
        text: String,
        speech: bool,
    },
    /// Recording or transcription failed.
    Failed {
        reason: String,
    },
    /// The cleanup brain answered (or failed).
    Cleaned(Result<String, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Orb {
    Hidden,
    Listening,
    Thinking,
    Done,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    StartRecording,
    StopRecording {
        transcribe: bool,
    },
    Orb(Orb),
    /// Ask the dictation brain to clean up this transcript.
    Cleanup(String),
    Insert(String),
}

pub const NO_SPEECH: &str = "I didn't hear anything.";

/// One transition. `cleanup` says whether a cleanup preset is active.
pub fn step(state: &State, input: Input, cfg: Config, cleanup: bool) -> (State, Vec<Output>) {
    use Input::*;
    use Output::*;
    match (state, input) {
        (State::Idle, Press { at_ms }) => (
            State::Recording { started_ms: at_ms },
            vec![StartRecording, Orb(self::Orb::Listening)],
        ),
        (State::Recording { started_ms }, Release { at_ms }) => {
            if at_ms.saturating_sub(*started_ms) < cfg.min_hold_ms && cfg.tap_to_toggle {
                (
                    State::Latched {
                        started_ms: *started_ms,
                    },
                    vec![],
                )
            } else if at_ms.saturating_sub(*started_ms) < cfg.min_hold_ms {
                (
                    State::Idle,
                    vec![StopRecording { transcribe: false }, Orb(self::Orb::Hidden)],
                )
            } else {
                (
                    State::Transcribing,
                    vec![StopRecording { transcribe: true }, Orb(self::Orb::Thinking)],
                )
            }
        }
        // A second press ends a latched recording (a near-instant double tap is still discarded).
        (State::Latched { started_ms }, Press { at_ms }) => {
            if at_ms.saturating_sub(*started_ms) < cfg.min_hold_ms {
                (
                    State::Idle,
                    vec![StopRecording { transcribe: false }, Orb(self::Orb::Hidden)],
                )
            } else {
                (
                    State::Transcribing,
                    vec![StopRecording { transcribe: true }, Orb(self::Orb::Thinking)],
                )
            }
        }
        (State::Recording { started_ms } | State::Latched { started_ms }, Tick { at_ms })
            if at_ms.saturating_sub(*started_ms) >= cfg.max_ms =>
        {
            (
                State::Transcribing,
                vec![StopRecording { transcribe: true }, Orb(self::Orb::Thinking)],
            )
        }
        (State::Transcribing, Transcript { text, speech }) => {
            let text = text.trim().to_string();
            if !speech || text.is_empty() {
                (State::Idle, vec![Orb(self::Orb::Error(NO_SPEECH.into()))])
            } else if cleanup {
                (State::Cleaning { raw: text.clone() }, vec![Cleanup(text)])
            } else {
                (State::Idle, vec![Insert(text), Orb(self::Orb::Done)])
            }
        }
        // P2: a failed or empty cleanup falls back to the raw transcript.
        (State::Cleaning { raw }, Cleaned(result)) => {
            let text = match result {
                Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
                _ => raw.clone(),
            };
            (State::Idle, vec![Insert(text), Orb(self::Orb::Done)])
        }
        (State::Recording { .. } | State::Latched { .. }, Failed { reason }) => (
            State::Idle,
            vec![
                StopRecording { transcribe: false },
                Orb(self::Orb::Error(reason)),
            ],
        ),
        (State::Transcribing, Failed { reason }) => {
            (State::Idle, vec![Orb(self::Orb::Error(reason))])
        }
        // P2 again: a failure while cleaning still inserts what was said.
        (State::Cleaning { raw }, Failed { .. }) => {
            (State::Idle, vec![Insert(raw.clone()), Orb(self::Orb::Done)])
        }
        // Everything else (key repeat, a release without a press, late results) changes nothing.
        (s, _) => (s.clone(), vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn run(inputs: Vec<Input>, cleanup: bool) -> (State, Vec<Output>) {
        let mut state = State::Idle;
        let mut out = vec![];
        for i in inputs {
            let (s, o) = step(&state, i, Config::default(), cleanup);
            state = s;
            out.extend(o);
        }
        (state, out)
    }

    #[test]
    fn hold_speak_release_inserts_the_transcript() {
        let (s, out) = run(
            vec![
                Input::Press { at_ms: 0 },
                Input::Release { at_ms: 1200 },
                Input::Transcript {
                    text: " Hello there. ".into(),
                    speech: true,
                },
            ],
            false,
        );
        assert_eq!(s, State::Idle);
        assert_eq!(
            out,
            vec![
                Output::StartRecording,
                Output::Orb(Orb::Listening),
                Output::StopRecording { transcribe: true },
                Output::Orb(Orb::Thinking),
                Output::Insert("Hello there.".into()),
                Output::Orb(Orb::Done),
            ]
        );
    }

    #[test]
    fn a_tap_is_discarded_and_silence_says_so() {
        let (_, out) = run(
            vec![Input::Press { at_ms: 0 }, Input::Release { at_ms: 249 }],
            false,
        );
        assert!(out.contains(&Output::StopRecording { transcribe: false }));
        let (_, out) = run(
            vec![
                Input::Press { at_ms: 0 },
                Input::Release { at_ms: 900 },
                Input::Transcript {
                    text: String::new(),
                    speech: false,
                },
            ],
            false,
        );
        assert_eq!(out.last(), Some(&Output::Orb(Orb::Error(NO_SPEECH.into()))));
    }

    #[test]
    fn cleanup_result_or_fallback_is_inserted() {
        let base = || {
            vec![
                Input::Press { at_ms: 0 },
                Input::Release { at_ms: 800 },
                Input::Transcript {
                    text: "um so hello".into(),
                    speech: true,
                },
            ]
        };
        let mut ok = base();
        ok.push(Input::Cleaned(Ok("Hello.".into())));
        assert!(run(ok, true).1.contains(&Output::Insert("Hello.".into())));
        let mut failed = base();
        failed.push(Input::Cleaned(Err("brain asleep".into())));
        assert!(run(failed, true)
            .1
            .contains(&Output::Insert("um so hello".into())));
    }

    #[test]
    fn the_cap_stops_a_long_recording() {
        let (s, out) = run(
            vec![
                Input::Press { at_ms: 0 },
                Input::Tick { at_ms: 299_999 },
                Input::Tick { at_ms: 300_000 },
            ],
            false,
        );
        assert_eq!(s, State::Transcribing);
        assert_eq!(
            out.iter()
                .filter(|o| matches!(o, Output::StopRecording { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn a_tap_latches_when_toggle_is_on_and_the_next_press_stops() {
        let cfg = Config {
            tap_to_toggle: true,
            ..Config::default()
        };
        let mut state = State::Idle;
        let mut out = vec![];
        for i in [
            Input::Press { at_ms: 0 },
            Input::Release { at_ms: 120 },
            Input::Release { at_ms: 130 },
            Input::Press { at_ms: 4_000 },
            Input::Release { at_ms: 4_100 },
            Input::Transcript {
                text: "Hands free.".into(),
                speech: true,
            },
        ] {
            let (s, o) = step(&state, i, cfg, false);
            if out.is_empty() && o.is_empty() {
                assert!(matches!(s, State::Latched { .. }) || s == State::Idle);
            }
            state = s;
            out.extend(o);
        }
        assert_eq!(state, State::Idle);
        assert_eq!(
            out.iter()
                .filter(|o| matches!(o, Output::StopRecording { transcribe: true }))
                .count(),
            1
        );
        assert!(out.contains(&Output::Insert("Hands free.".into())));
        // A long hold still behaves as push-to-talk.
        let (s, _) = step(
            &State::Recording { started_ms: 0 },
            Input::Release { at_ms: 900 },
            cfg,
            false,
        );
        assert_eq!(s, State::Transcribing);
    }

    fn arb_input() -> impl Strategy<Value = (u8, u64, bool, bool)> {
        (0u8..7, 0u64..400_000, any::<bool>(), any::<bool>())
    }

    proptest! {
        /// P1 (VD-4, VD-11): for any event sequence with a monotonic clock,
        /// - text is inserted only for holds of at least `min_hold_ms`,
        /// - a recording never runs past the cap without a stop (given ticks),
        /// - every start is matched by exactly one stop, and
        /// - after the results arrive the machine is Idle.
        #[test]
        fn p1_holds_cap_and_termination(raw in proptest::collection::vec(arb_input(), 0..60), cleanup in any::<bool>(), toggle in any::<bool>()) {
            let cfg = Config { tap_to_toggle: toggle, ..Config::default() };
            let mut state = State::Idle;
            let mut now = 0u64;
            let mut last_hold: Option<u64> = None;
            let (mut starts, mut stops) = (0, 0);
            for (kind, dt, speech, ok) in raw {
                now += dt % 5_000;
                let input = match kind {
                    0 => Input::Press { at_ms: now },
                    1 => Input::Release { at_ms: now },
                    2 => Input::Tick { at_ms: now },
                    3 => Input::Transcript { text: if speech { "words".into() } else { String::new() }, speech },
                    4 => Input::Failed { reason: "x".into() },
                    5 => Input::Cleaned(if ok { Ok("clean".into()) } else { Err("e".into()) }),
                    _ => Input::Tick { at_ms: now + cfg.max_ms },
                };
                let mut cap_hit = false;
                if let (State::Recording { started_ms } | State::Latched { started_ms }, Input::Tick { at_ms }) = (&state, &input) {
                    if at_ms - started_ms >= cfg.max_ms { last_hold = Some(at_ms - started_ms); cap_hit = true; }
                }
                if let (State::Recording { started_ms }, Input::Release { at_ms }) = (&state, &input) {
                    last_hold = Some(at_ms - started_ms);
                }
                if let (State::Latched { started_ms }, Input::Press { at_ms }) = (&state, &input) {
                    last_hold = Some(at_ms - started_ms);
                }
                let (next, out) = step(&state, input, cfg, cleanup);
                for o in &out {
                    match o {
                        Output::StartRecording => starts += 1,
                        Output::StopRecording { .. } => stops += 1,
                        Output::Insert(t) => {
                            prop_assert!(!t.trim().is_empty());
                            prop_assert!(last_hold.unwrap_or(0) >= cfg.min_hold_ms, "inserted after a short hold");
                        }
                        _ => {}
                    }
                }
                if cap_hit {
                    prop_assert!(!matches!(next, State::Recording { .. } | State::Latched { .. }), "recording continued past the cap");
                }
                state = next;
            }
            prop_assert!(stops <= starts && starts <= stops + 1);
            // Drain: release (or a second press when latched), a transcript and a cleanup result always bring it home.
            if matches!(state, State::Latched { .. }) {
                state = step(&state, Input::Press { at_ms: now + cfg.max_ms }, cfg, cleanup).0;
            }
            for input in [
                Input::Release { at_ms: now + cfg.min_hold_ms },
                Input::Transcript { text: "done".into(), speech: true },
                Input::Cleaned(Err("e".into())),
            ] {
                state = step(&state, input, cfg, cleanup).0;
            }
            prop_assert_eq!(state, State::Idle);
        }

        /// P2 (VD-6): whatever the cleanup returns or however it fails, something non-empty is inserted, and
        /// it is the raw transcript unless the cleanup produced text.
        #[test]
        fn p2_cleanup_never_loses_the_transcript(raw in "[a-z ]{1,40}[a-z]", cleaned in proptest::option::of("\\PC{0,40}")) {
            let s = State::Cleaning { raw: raw.trim().to_string() };
            let input = match &cleaned { Some(c) => Input::Cleaned(Ok(c.clone())), None => Input::Cleaned(Err("down".into())) };
            let (_, out) = step(&s, input, Config::default(), true);
            let inserted = out.iter().find_map(|o| if let Output::Insert(t) = o { Some(t.clone()) } else { None }).unwrap();
            match cleaned {
                Some(c) if !c.trim().is_empty() => prop_assert_eq!(inserted, c.trim().to_string()),
                _ => prop_assert_eq!(inserted, raw.trim().to_string()),
            }
        }
    }
}
