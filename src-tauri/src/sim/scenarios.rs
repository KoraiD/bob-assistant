//! One scenario per suspected cause of wrong Ghost Text insertions (review hypotheses H1 to H8, findings
//! `GT-nn` in the review). Each asserts what the user should get. Scenarios that fail today are ignored with
//! the finding ID until their fix lands; `cargo test sim::scenarios -- --ignored` shows them failing.

use super::actor::{Script, Step};
use super::model::Model;
use super::world::{Profile, CHROME, ELECTRON, TEXTEDIT, TEXTEDIT_SMART};
use super::{run, Config, Report};
use crate::protocol::Key;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const TYPED: &str = "Thanks for the update! I will get back to";
const ANSWER: (&str, &str) = ("I will get back to", " you as soon as I can.");

async fn play(seed: u64, profile: Profile, model: Model, steps: Vec<Step>) -> Report {
    play_with(Config::new(seed, profile, model), steps).await
}

async fn play_with(config: Config, steps: Vec<Step>) -> Report {
    let r = run(config, &mut Script::new(steps)).await;
    eprintln!("{}", r.summary());
    for d in &r.defects {
        eprintln!("  defect at {} ms: {:?}: {}", d.at_ms, d.kind, d.detail);
    }
    r
}

fn assert_clean(r: &Report, text: &str) {
    assert!(
        r.defects.is_empty() && r.text == text,
        "expected {text:?}, got {:?} with {:?}\n{}",
        r.text,
        r.defects,
        r.timeline()
    );
}

fn type_and_wait() -> Vec<Step> {
    vec![
        Step::Type(TYPED, ms(180)),
        Step::WaitChip(ms(1500)),
        Step::Wait(ms(400)),
    ]
}

/// When the user pressed the first Tab, in µs since the start.
fn first_tab_us(r: &Report) -> u64 {
    r.log
        .iter()
        .find(|l| l.who == "user" && l.what.starts_with("Tab"))
        .map_or(0, |l| l.at_us)
}

/// H1 (GT-01): a second Tab before the app reported the first insertion inserts the same word again.
#[tokio::test(start_paused = true)]
async fn h1_a_quick_second_tab_takes_the_next_word() {
    for profile in [ELECTRON, CHROME] {
        let mut steps = type_and_wait();
        steps.extend([
            Step::Press(Key::Tab),
            Step::Wait(ms(120)),
            Step::Press(Key::Tab),
            Step::Wait(ms(800)),
        ]);
        let r = play(1, profile, Model::Table(vec![ANSWER]), steps).await;
        assert_clean(&r, &format!("{TYPED} you as"));
    }
}

/// H1, second trigger: typing the chip's first letters and pressing Tab before the app reported them.
#[tokio::test(start_paused = true)]
async fn h1_tab_right_after_typing_part_of_the_chip() {
    for seed in 1..=5 {
        let mut steps = type_and_wait();
        steps.extend([
            Step::TypeChip(2, ms(150)),
            Step::Press(Key::Tab),
            Step::Wait(ms(800)),
        ]);
        let r = play(seed, ELECTRON, Model::Table(vec![ANSWER]), steps).await;
        assert_clean(&r, &format!("{TYPED} you"));
    }
}

/// H2 (GT-02): typing through a chip while it still streams freezes it at a part of a word.
#[tokio::test(start_paused = true)]
async fn h2_typing_through_a_streaming_chip_never_leaves_a_fragment() {
    let model = Model::Paced {
        table: vec![("about the", " possibility of a meeting next week")],
        first_ms: 40,
        gap_ms: 80,
    };
    let steps = vec![
        Step::Type("Let me know about the", ms(180)),
        Step::WaitChip(ms(1500)),
        Step::TypeChip(2, ms(100)),
        Step::Wait(ms(800)),
        Step::Press(Key::Tab),
        Step::Wait(ms(600)),
    ];
    let r = play(1, TEXTEDIT, model, steps).await;
    assert_clean(&r, "Let me know about the possibility");
}

/// H3 (GT-03): a token that arrives after Tab, before the app reported the insertion, paints the whole chip
/// again ("to you" + " you as soon" on screen), and the next Tab inserts the accepted word a second time.
#[tokio::test(start_paused = true)]
async fn h3_a_token_after_tab_does_not_bring_the_word_back() {
    let model = Model::Paced {
        table: vec![ANSWER],
        first_ms: 40,
        gap_ms: 60,
    };
    let steps = vec![
        Step::Type(TYPED, ms(180)),
        Step::WaitChip(ms(1500)),
        Step::Press(Key::Tab),
        Step::Wait(ms(150)),
        Step::Press(Key::Tab),
        Step::Wait(ms(800)),
    ];
    let r = play(1, ELECTRON, model, steps).await;
    assert_clean(&r, &format!("{TYPED} you as"));
    let tab = first_tab_us(&r);
    let repainted = r
        .log
        .iter()
        .any(|l| l.at_us > tab && l.who == "chip" && l.what.starts_with("shows \" you"));
    assert!(!repainted, "the accepted word came back on screen");
}

/// H4 (GT-04): Backspace soon after an accept is taken for a late report of the old text and ignored, so the
/// chip keeps showing the wrong continuation and the next Tab inserts it. TextEdit reports the insertion
/// within milliseconds, which ends the stale window early; in Electron apps one Accessibility update can carry
/// both the insertion and the Backspace.
#[tokio::test(start_paused = true)]
async fn h4_backspace_right_after_an_accept_is_seen() {
    // Seed 34: the insertion and the first Backspace reach Accessibility together (1 in 40 Electron seeds).
    for (profile, seed) in [(TEXTEDIT, 1), (ELECTRON, 34)] {
        let mut steps = type_and_wait();
        steps.extend([
            Step::Press(Key::Tab),
            Step::Wait(ms(150)),
            Step::Backspace,
            Step::Backspace,
            Step::Wait(ms(1200)),
            Step::Press(Key::Tab),
            Step::Wait(ms(600)),
        ]);
        let r = play(seed, profile, Model::Table(vec![ANSWER]), steps).await;
        assert_clean(&r, &format!("{TYPED} you"));
    }
}

/// H5 (GT-05): Tab on a suggestion inside a sentence drops the space the suggestion owns ("latestreport").
#[tokio::test(start_paused = true)]
async fn h5_a_gap_suggestion_keeps_its_space() {
    let text = "Send me the report today.";
    let config = Config::new(1, TEXTEDIT, Model::Table(vec![("Send me the ", "latest]")]))
        .with_text(text, Some("Send me the ".len()));
    let steps = vec![
        Step::WaitChip(ms(1500)),
        Step::Wait(ms(300)),
        Step::Press(Key::Tab),
        Step::Wait(ms(600)),
    ];
    let r = play_with(config, steps).await;
    assert_clean(&r, "Send me the latest report today.");
}

/// H6 (GT-06): the app rewrites what BOB types (smart quotes turn "don't" into "don’t"). The rest of the chip
/// must survive without asking the model again; today the rewrite looks like the user typing something else,
/// so the chip disappears and the next Tab reaches the app unless a new suggestion arrived in time.
#[tokio::test(start_paused = true)]
async fn h6_the_chip_survives_smart_quotes() {
    let steps = vec![
        Step::Type("Honestly I", ms(180)),
        Step::WaitChip(ms(1500)),
        Step::Wait(ms(400)),
        Step::Press(Key::Tab),
        Step::Wait(ms(500)),
        Step::Press(Key::Tab),
        Step::Wait(ms(600)),
    ];
    // A real model also answers for the rewritten text, and so does the table: the text can come out right
    // either way. What matters is whether the chip survived.
    let model = Model::Table(vec![
        ("Honestly I", " don't know what to say"),
        ("Honestly I don’t", " know what to say"),
    ]);
    let r = play(1, TEXTEDIT_SMART, model, steps).await;
    assert_clean(&r, "Honestly I don’t know");
    let tab = first_tab_us(&r);
    let hidden = r
        .log
        .iter()
        .filter(|l| l.at_us > tab && l.at_us < tab + 500_000 && l.what == "hidden")
        .count();
    let asked = r
        .prompts
        .iter()
        .filter(|(_, p)| p.ends_with("don’t"))
        .count();
    assert!(
        hidden == 0 && asked == 0,
        "after the rewrite: chip hidden {hidden} times, {asked} new requests"
    );
}

/// H7 (GT-07): the Deletes of a typo fix look like typing; nothing should be requested while the fix is being
/// typed.
#[tokio::test(start_paused = true)]
async fn h7_a_typo_fix_asks_nothing_for_the_half_deleted_word() {
    let steps = vec![
        Step::Type("We will recieve ", ms(180)),
        Step::WaitChip(ms(1500)),
        Step::Press(Key::Tab),
        Step::Wait(ms(1000)),
    ];
    let r = play(1, ELECTRON, Model::Table(vec![]), steps).await;
    assert_clean(&r, "We will receive ");
    let tab = first_tab_us(&r) / 1000;
    let half: Vec<&String> = r
        .prompts
        .iter()
        .filter(|(at, p)| *at >= tab && p != "We will receive ")
        .map(|(_, p)| p)
        .collect();
    assert!(
        half.is_empty(),
        "requests while the fix was being typed: {half:?}\n{}",
        r.timeline()
    );
}

/// A typo fix while the user keeps typing: the helper checks the word against what Accessibility shows, which
/// lags in Electron apps, then deletes from the real text. Found while writing H7 (GT-09).
#[tokio::test(start_paused = true)]
async fn h7b_a_typo_fix_never_deletes_what_the_user_typed_after_it() {
    for seed in 1..=10 {
        let steps = vec![
            Step::Type("We will recieve ", ms(180)),
            Step::WaitChip(ms(1500)),
            Step::Type("t", ms(150)),
            Step::Press(Key::Tab),
            Step::Wait(ms(1000)),
        ];
        let r = play(seed, ELECTRON, Model::Table(vec![]), steps).await;
        assert!(
            r.defects.is_empty() && r.text.starts_with("We will re") && !r.text.contains("rre"),
            "seed {seed}: {:?} {:?}\n{}",
            r.text,
            r.defects,
            r.timeline()
        );
    }
}

/// H8 (GT-08): past 2000 characters the helper's text window slides with every key, so typing through the
/// chip no longer matches and every key hides the chip and asks the model again.
#[tokio::test(start_paused = true)]
async fn h8_typing_through_works_in_a_long_document() {
    let long =
        "The quarterly report covers sales, hiring and the budget for next year. ".repeat(30);
    let config = Config::new(1, TEXTEDIT, Model::Table(vec![ANSWER])).with_text(long.clone(), None);
    let mut steps = type_and_wait();
    steps.extend([Step::TypeChip(3, ms(180)), Step::Wait(ms(300))]);
    let r = play_with(config, steps).await;
    let first_chip = r
        .log
        .iter()
        .position(|l| l.who == "chip")
        .unwrap_or(r.log.len());
    let hidden = r.log[first_chip..]
        .iter()
        .filter(|l| l.what == "hidden")
        .count();
    let shown_at = r.log.get(first_chip).map_or(0, |l| l.at_us / 1000);
    let asked_again = r.prompts.iter().filter(|(at, _)| *at > shown_at).count();
    assert!(
        r.defects.is_empty()
            && hidden == 0
            && asked_again == 0
            && r.text == format!("{long}{TYPED} yo"),
        "hidden {hidden} times, asked again {asked_again} times, defects {:?}",
        r.defects
    );
}

/// GT-10 (found by the random sessions): after typing part of a chip, the left arrow brings the chip back as
/// if the caret were still at the end, ignoring the text now after the caret; Tab glues it on ("morningm").
#[tokio::test(start_paused = true)]
async fn gt10_moving_the_caret_back_does_not_revive_the_chip() {
    let mut steps = type_and_wait();
    steps.extend([
        Step::TypeChip(2, ms(180)),
        Step::Caret(-1),
        Step::Wait(ms(300)),
        Step::Press(Key::Tab),
        Step::Wait(ms(800)),
    ]);
    let r = play(1, TEXTEDIT, Model::Table(vec![ANSWER]), steps).await;
    assert!(
        r.defects.is_empty() && !r.text.contains("youy"),
        "{:?} {:?}",
        r.text,
        r.defects
    );
}

/// GT-11 (found by the random sessions): the engine stops the stream at the sixth word, even when that word
/// is only half streamed (models split long words into several tokens: " repo" + "rt"), so the chip ends in
/// the middle of a word. The sixth word must be finished first.
#[tokio::test(start_paused = true)]
async fn gt11_the_word_cap_never_cuts_a_word() {
    let model = Model::Table(vec![(
        "Please review",
        " the latest version of the report today.",
    )]);
    let steps = vec![
        Step::Type("Please review", ms(180)),
        Step::WaitChip(ms(1500)),
        Step::Wait(ms(400)),
        Step::Press(Key::ShiftTab),
        Step::Wait(ms(600)),
    ];
    let r = play(1, TEXTEDIT, model, steps).await;
    assert_clean(&r, "Please review the latest version of the report");
}
