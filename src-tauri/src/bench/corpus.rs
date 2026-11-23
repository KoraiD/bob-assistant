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
- **Name:** Dávid\n\
- **Role:** PhD student and software engineer\n\
- I write friendly, concise emails and chat messages in English.\n\
- Sign-off: Best, Dávid\n";

/// Seed memory for the context benchmark: phrases a user might have accepted before. Six recur in the
/// corpus and six do not, so the memory layer is measured both helping and distracting.
pub const MEMORY: &[&str] = &[
    "as soon as possible",
    "let me know if you have any questions",
    "running a bit late",
    "Thank you so much for your help",
    "Thursday afternoon",
    "Happy birthday!",
    "Best, Dávid",
    "see you on Monday",
    "the latest build",
    "sounds good to me",
    "the quarterly review",
    "on my way",
];
