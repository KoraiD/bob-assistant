use super::actor::{Script, Step};
use super::model::{Model, Quirks};
use super::world::{expected_insert, CHROME, ELECTRON, TEXTEDIT};
use super::{run, Config, Report};
use crate::protocol::Key;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const TYPED: &str = "Thanks for the update! I will get back to";

fn table() -> Model {
    Model::Table(vec![("I will get back to", " you as soon as I can.")])
}

/// Type, wait for the chip, Tab for one word, Shift+Tab for the rest.
fn accept_word_then_rest() -> Script {
    Script::new([
        Step::Type(TYPED, ms(180)),
        Step::WaitChip(ms(1500)),
        Step::Wait(ms(400)),
        Step::Press(Key::Tab),
        Step::Wait(ms(500)),
        Step::Press(Key::ShiftTab),
        Step::Wait(ms(800)),
    ])
}

fn print(r: &Report, timeline: bool) {
    eprintln!("{}", r.summary());
    if timeline {
        eprintln!("{}", r.timeline());
    }
    for d in &r.defects {
        eprintln!("  defect at {} ms: {:?}: {}", d.at_ms, d.kind, d.detail);
    }
}

#[tokio::test(start_paused = true)]
async fn textedit_type_pause_tab_and_shift_tab() {
    let r = run(
        Config::new(1, TEXTEDIT, table()),
        &mut accept_word_then_rest(),
    )
    .await;
    print(&r, true);
    assert!(r.defects.is_empty(), "{:?}", r.defects);
    // The chip stops at the six-word cap (`chip::MAX_WORDS`), before the model's final period.
    assert_eq!(r.text, format!("{TYPED} you as soon as I can"));
    assert_eq!(r.stats.accepts, 2);
    // Typing at 180 ms per key cancels most requests before the quiet gate; the last one streams.
    assert!(
        r.requests >= 1 && r.stats.chips_painted >= 1,
        "{}",
        r.summary()
    );
}

/// The same session in an app that reports text late and the caret first (the Kiro editor, VS Code). Task 2
/// only shows that it runs and reports; Task 3 turns the defects it finds into regression tests.
#[tokio::test(start_paused = true)]
async fn electron_and_chrome_timing_sessions_run_and_report() {
    for profile in [ELECTRON, CHROME] {
        let r = run(
            Config::new(1, profile, table()),
            &mut accept_word_then_rest(),
        )
        .await;
        print(&r, false);
        assert!(r.stats.keys as usize >= TYPED.chars().count());
        assert!(r.text.starts_with(TYPED), "{:?}", r.text);
        assert!(!r.log.is_empty());
    }
}

/// One seed, one session: the virtual clock and seeded delays replay a session exactly.
#[tokio::test(start_paused = true)]
async fn a_seed_replays_the_same_session() {
    let session = |seed| async move {
        let script = Script::new([
            Step::Type("I will get back to you about thx", ms(150)),
            Step::Backspace,
            Step::Type("e", ms(150)),
            Step::WaitChip(ms(1000)),
            Step::TypeChip(3, ms(160)),
            Step::Press(Key::Tab),
            Step::Wait(ms(300)),
            Step::Press(Key::ShiftTab),
            Step::Wait(ms(600)),
        ]);
        let mut script = script;
        run(
            Config::new(seed, ELECTRON, Model::Random(Quirks::default())),
            &mut script,
        )
        .await
    };
    let (a, b) = (session(42).await, session(42).await);
    assert_eq!(a.timeline(), b.timeline());
    assert_eq!(a.text, b.text);
    assert_eq!(a.defects, b.defects);
    let c = session(43).await;
    assert_ne!(
        a.timeline(),
        c.timeline(),
        "a different seed must change the session"
    );
}

#[test]
fn what_an_accept_should_insert() {
    let claim = "I will get back to you as soon as I can.";
    assert_eq!(
        expected_insert(claim, "I will get back to", Key::Tab).as_deref(),
        Some(" you")
    );
    assert_eq!(
        expected_insert(claim, "I will get back to you", Key::ShiftTab).as_deref(),
        Some(" as soon as I can.")
    );
    // Typed through part of a word: the rest of that word.
    assert_eq!(
        expected_insert(claim, "I will get back to y", Key::Tab).as_deref(),
        Some("ou")
    );
    // The chip no longer continues the text.
    assert_eq!(
        expected_insert(claim, "I will get back to x", Key::Tab),
        None
    );
    assert_eq!(expected_insert(claim, claim, Key::Tab), None);
    // The last word takes the space the chip owns (a gap chip before "report").
    assert_eq!(
        expected_insert("Send me the latest ", "Send me the ", Key::Tab).as_deref(),
        Some("latest ")
    );
}
