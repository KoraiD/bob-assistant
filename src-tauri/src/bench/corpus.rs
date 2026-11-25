//! Benchmark sentences. Written for this project (no third-party text), covering everyday writing where
//! inline suggestions matter: email, chat, notes and documents. Some are predictable, some are not, so the
//! benchmark rewards models that help without inventing.

pub const SENTENCES: &[(&str, &str)] = &[
    (
        "email-reply",
        "Thanks for your email, I will get back to you as soon as possible.",
    ),
    (
        "email-meeting",
        "Could we move our meeting to Thursday afternoon instead?",
    ),
    (
        "email-attach",
        "Please find the attached report and let me know if you have any questions.",
    ),
    (
        "chat-late",
        "Sorry, I am running a bit late but I will be there in ten minutes.",
    ),
    (
        "chat-thanks",
        "Thank you so much for your help with the move last weekend.",
    ),
    (
        "chat-plan",
        "Do you want to grab lunch together tomorrow at noon?",
    ),
    (
        "note-todo",
        "Remember to call the dentist and renew the car insurance this week.",
    ),
    (
        "note-idea",
        "The demo should start with the problem before showing any features.",
    ),
    (
        "doc-results",
        "The results of the experiment show a clear improvement over the baseline.",
    ),
    (
        "doc-intro",
        "In this paper we propose a simple method for faster inline text prediction.",
    ),
    (
        "birthday",
        "Happy birthday! I hope you have a wonderful day with your family.",
    ),
    (
        "support",
        "I have tried restarting the app but the problem still happens every time.",
    ),
];

/// A plausible window title per trace (window layer of the context benchmark).
pub fn window_title(trace_id: &str) -> Option<&'static str> {
    let kind = trace_id.split('-').next().unwrap_or("");
    Some(match kind {
        "email" => "Mail: Re: Project update",
        "chat" => "Messages: Anna",
        "note" => "Notes: This week",
        "doc" => "Pages: Paper draft",
        "birthday" => "Messages: Mom",
        "support" => "Safari: Contact support",
        _ => return None,
    })
}

/// Sample profile for the context benchmark (written for this project).
pub const PROFILE: &str = "# About me\n\
- **Name:** Zsófi\n\
- **Role:** PhD student and software engineer\n\
- I write friendly, concise emails and chat messages in English.\n\
- Sign-off: Best, Zsófi\n";

/// Seed memory for the context benchmark: phrases a user might have accepted before. Six recur in the
/// corpus and six do not, so the memory layer is measured both helping and distracting.
pub const MEMORY: &[&str] = &[
    "as soon as possible",
    "let me know if you have any questions",
    "running a bit late",
    "Thank you so much for your help",
    "Thursday afternoon",
    "Happy birthday!",
    "Best, Zsófi",
    "see you on Monday",
    "the latest build",
    "sounds good to me",
    "the quarterly review",
    "on my way",
];

/// What might be visible on screen while typing each trace (screen layer of the context benchmark): the
/// message being answered or the page being worked on, plus the menu and sidebar noise that OCR also picks
/// up. Written for this project.
pub fn screen_text(trace_id: &str) -> Option<&'static str> {
    Some(match trace_id {
        "email-reply" => "Inbox Sent Drafts\nRe: Project update\nHi, could you check the numbers in the draft? \
                          No rush, but I need them before Friday. Thanks, Anna\nReply Forward Archive",
        "email-meeting" => "Calendar Week 48\nThursday 14:00 Free\nMonday Team sync\nRe: Planning\nCan we find a \
                            better time for our meeting? Monday does not work for me.",
        "email-attach" => "New Message\nTo: finance team\nSubject: Q3 report\nAttachments: report-q3.pdf (2 MB)\nSend",
        "chat-late" => "Messages\nAnna: Where are you? We are all at the cafe already.\nToday 18:05",
        "chat-thanks" => "Messages\nTom: No problem at all, it was fun moving all those boxes last weekend!",
        "chat-plan" => "Messages\nSara: I'm free tomorrow, lunch maybe? Noon works for me.",
        "note-todo" => "Notes\nThis week\nDentist appointment - call to confirm\nCar insurance renewal due Friday",
        "note-idea" => "Notes\nDemo script\n1. Problem first\n2. Then the features\n3. Close with the ghost",
        "doc-results" => "Pages\nResults\nFigure 2: accuracy over the baseline improved from 61% to 74%\nTable 1",
        "doc-intro" => "Pages\nAbstract\nWe propose a simple method for faster inline text prediction on laptops.",
        "birthday" => "Messages\nMom: Today is a wonderful day, we are having dinner with the family tonight!",
        "support" => "Safari\nContact support\nDescribe the problem. Did you try restarting the app?\nSubmit",
        _ => return None,
    })
}
