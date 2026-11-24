//! Preferred writing languages (Personalize > Writing style).
//!
//! Small models switch languages inside one script: after the Hungarian "Hát." the default model suggested
//! Vietnamese ("100% là một album rất hay", owner's screenshot). The script check in `ghost::chip` cannot see
//! that, because both are Latin. This module knows each language's letters, so a chip with letters none of
//! the user's languages use is cut before that word, or dropped.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Lang {
    pub code: &'static str,
    pub name: &'static str,
    /// Letters beyond A to Z (lowercase), for Latin-script languages. Non-Latin scripts are checked by
    /// script in `ghost::chip`.
    pub letters: &'static str,
    /// Frequent short words, for the benchmark's language detector.
    pub common: &'static [&'static str],
}

pub const LANGS: &[Lang] = &[
    Lang {
        code: "en",
        name: "English",
        letters: "",
        common: &[
            "the", "and", "you", "to", "is", "for", "that", "with", "it", "of", "i", "am", "are",
            "we", "my", "because", "this", "have", "not", "be", "will", "can", "your", "our",
        ],
    },
    Lang {
        code: "hu",
        name: "Hungarian",
        letters: "áéíóöőúüű",
        common: &[
            "a",
            "az",
            "és",
            "hogy",
            "nem",
            "is",
            "egy",
            "van",
            "meg",
            "már",
            "de",
            "hát",
            "mert",
            "vagyok",
            "vagy",
            "kell",
            "csak",
            "még",
            "ez",
            "volt",
            "lesz",
            "nincs",
            "jó",
            "szia",
            "köszönöm",
            "holnap",
            "ma",
            "majd",
            "nagyon",
            "mi",
            "én",
            "te",
        ],
    },
    Lang {
        code: "de",
        name: "German",
        letters: "äöüß",
        common: &[
            "der", "die", "und", "ist", "nicht", "ich", "das", "zu", "mit", "ein",
        ],
    },
    Lang {
        code: "fr",
        name: "French",
        letters: "àâæçéèêëîïôœùûüÿ",
        common: &[
            "le", "la", "et", "les", "des", "est", "pas", "je", "vous", "une",
        ],
    },
    Lang {
        code: "es",
        name: "Spanish",
        letters: "áéíñóúü",
        common: &[
            "el", "la", "que", "y", "de", "los", "es", "por", "una", "con",
        ],
    },
    Lang {
        code: "it",
        name: "Italian",
        letters: "àèéìíîòóùú",
        common: &[
            "il", "che", "di", "la", "e", "non", "per", "una", "sono", "con",
        ],
    },
    Lang {
        code: "pt",
        name: "Portuguese",
        letters: "áâãàçéêíóôõú",
        common: &[
            "o", "que", "de", "não", "uma", "para", "com", "os", "é", "em",
        ],
    },
    Lang {
        code: "nl",
        name: "Dutch",
        letters: "éëïóöü",
        common: &[
            "de", "het", "een", "en", "van", "niet", "ik", "dat", "is", "op",
        ],
    },
    Lang {
        code: "pl",
        name: "Polish",
        letters: "ąćęłńóśźż",
        common: &["i", "nie", "się", "w", "na", "to", "że", "jest", "z", "do"],
    },
    Lang {
        code: "ro",
        name: "Romanian",
        letters: "ăâîșşțţ",
        common: &["și", "de", "la", "nu", "în", "este", "că", "pe", "cu", "o"],
    },
    Lang {
        code: "cs",
        name: "Czech",
        letters: "áčďéěíňóřšťúůýž",
        common: &["a", "je", "se", "na", "že", "to", "v", "není", "jsem", "s"],
    },
    Lang {
        code: "sk",
        name: "Slovak",
        letters: "áäčďéíĺľňóôŕšťúýž",
        common: &["a", "je", "sa", "na", "že", "to", "v", "nie", "som", "s"],
    },
    Lang {
        code: "sv",
        name: "Swedish",
        letters: "åäö",
        common: &[
            "och", "att", "det", "är", "jag", "inte", "en", "på", "för", "med",
        ],
    },
    Lang {
        code: "da",
        name: "Danish",
        letters: "æøå",
        common: &[
            "og", "at", "det", "er", "jeg", "ikke", "en", "på", "for", "med",
        ],
    },
    Lang {
        code: "no",
        name: "Norwegian",
        letters: "æøå",
        common: &[
            "og", "at", "det", "er", "jeg", "ikke", "en", "på", "for", "med",
        ],
    },
    Lang {
        code: "fi",
        name: "Finnish",
        letters: "äö",
        common: &[
            "ja", "on", "ei", "että", "se", "olen", "mutta", "kun", "niin", "hän",
        ],
    },
    Lang {
        code: "tr",
        name: "Turkish",
        letters: "çğıöşü",
        common: &[
            "ve", "bir", "bu", "için", "ne", "değil", "çok", "da", "ile", "ben",
        ],
    },
    Lang {
        code: "vi",
        name: "Vietnamese",
        letters: "àáâãèéêìíòóôõùúýăđơưạảấầẩẫậắằẳẵặẹẻẽếềểễệỉịọỏốồổỗộớờởỡợụủứừửữựỳỵỷỹ",
        common: &[
            "là", "và", "của", "có", "không", "một", "những", "được", "cho", "rất",
        ],
    },
    Lang {
        code: "ru",
        name: "Russian",
        letters: "",
        common: &["и", "в", "не", "на", "что", "я", "с", "он", "как", "это"],
    },
    Lang {
        code: "uk",
        name: "Ukrainian",
        letters: "",
        common: &["і", "в", "не", "на", "що", "я", "з", "це", "як", "та"],
    },
    Lang {
        code: "el",
        name: "Greek",
        letters: "",
        common: &["και", "το", "να", "η", "ο", "της", "δεν", "με", "για", "τα"],
    },
    Lang {
        code: "ja",
        name: "Japanese",
        letters: "",
        common: &[],
    },
    Lang {
        code: "zh",
        name: "Chinese",
        letters: "",
        common: &[],
    },
];

/// Most preferred languages a user can pick.
pub const MAX_PREFERRED: usize = 4;

pub fn find(code: &str) -> Option<&'static Lang> {
    LANGS.iter().find(|l| l.code == code)
}

/// Known codes only, deduplicated, in order, at most [`MAX_PREFERRED`].
pub fn sanitize(codes: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for c in codes {
        let c = c.trim().to_lowercase();
        if find(&c).is_some() && !out.contains(&c) && out.len() < MAX_PREFERRED {
            out.push(c);
        }
    }
    out
}

/// "Hungarian and English", "Hungarian, German and English".
pub fn names(codes: &[String]) -> String {
    let n: Vec<&str> = codes
        .iter()
        .filter_map(|c| find(c))
        .map(|l| l.name)
        .collect();
    match n.len() {
        0 => String::new(),
        1 => n[0].to_string(),
        _ => format!("{} and {}", n[..n.len() - 1].join(", "), n[n.len() - 1]),
    }
}

fn is_latin_extra(c: char) -> bool {
    let u = c as u32;
    c.is_alphabetic()
        && !c.is_ascii()
        && ((0x00C0..=0x024F).contains(&u) || (0x1E00..=0x1EFF).contains(&u))
}

/// Letters that, without any preference, are only used by Vietnamese among the supported languages.
fn vietnamese_only(c: char) -> bool {
    let u = c as u32;
    matches!(c, 'ơ' | 'ư' | 'Ơ' | 'Ư') || (0x1EA0..=0x1EF9).contains(&u)
}

/// Filter a polished chip for language. `preferred` are language codes (may be empty). Letters already in
/// `before` are always fine (the user is typing them). Returns the chip, a shorter chip cut before the first
/// word with an unexpected letter, or `None` when nothing worth showing is left.
///
/// Without preferences only Vietnamese-only letters count as unexpected, unless the user typed them.
pub fn filter_chip(chip: &str, before: &str, preferred: &[String]) -> Option<String> {
    let typed: std::collections::HashSet<char> =
        before.chars().flat_map(char::to_lowercase).collect();
    let allowed: Option<std::collections::HashSet<char>> = (!preferred.is_empty()).then(|| {
        preferred
            .iter()
            .filter_map(|c| find(c))
            .flat_map(|l| l.letters.chars())
            .collect()
    });
    let known = |c: char| {
        let lc = c.to_lowercase().next().unwrap_or(c);
        typed.contains(&lc) || allowed.as_ref().is_some_and(|a| a.contains(&lc))
    };
    let strictly_foreign = |c: char| {
        if !is_latin_extra(c) || known(c) {
            return false;
        }
        match &allowed {
            Some(_) => true,
            None => vietnamese_only(c),
        }
    };
    if !chip.chars().any(strictly_foreign) {
        // Letters cannot tell French from Hungarian ("é" is in both). With preferences, a chip made of
        // another language's common words and none of the user's is dropped too (style benchmark).
        if !preferred.is_empty() && foreign_words(chip, preferred) {
            return None;
        }
        return Some(chip.to_string());
    }
    // Cut before the first word with any letter outside what the user typed or prefers.
    let suspicious = |c: char| is_latin_extra(c) && !known(c);
    let mut cut = chip.len();
    let mut word_start = 0;
    let mut in_word = false;
    for (i, c) in chip.char_indices() {
        if c.is_whitespace() {
            in_word = false;
            continue;
        }
        if !in_word {
            in_word = true;
            word_start = i;
        }
        if suspicious(c) {
            cut = word_start;
            break;
        }
    }
    let kept = chip[..cut].trim_end();
    if !preferred.is_empty() && foreign_words(kept, preferred) {
        return None;
    }
    kept.chars()
        .any(char::is_alphabetic)
        .then(|| kept.to_string())
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// At least two words, at least one common word of a language the user does not write, none of theirs.
fn foreign_words(chip: &str, preferred: &[String]) -> bool {
    let words = words_of(chip);
    if words.len() < 2 {
        return false;
    }
    let hits = |l: &Lang| {
        words
            .iter()
            .filter(|w| l.common.contains(&w.as_str()))
            .count()
    };
    let ours = LANGS
        .iter()
        .filter(|l| preferred.iter().any(|p| p == l.code))
        .map(hits)
        .sum::<usize>();
    let theirs = LANGS
        .iter()
        .filter(|l| !preferred.iter().any(|p| p == l.code))
        .map(hits)
        .max()
        .unwrap_or(0);
    ours == 0 && theirs >= 1
}

/// A confident guess: at least two signals, and ahead of every other language. For checking that a rewrite
/// came back in the right language; `None` when unsure (short or mixed text).
pub fn detect_strong(text: &str) -> Option<&'static str> {
    let mut scores: Vec<(&'static str, usize)> =
        LANGS.iter().map(|l| (l.code, score(l, text))).collect();
    scores.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
    match (scores.first(), scores.get(1)) {
        (Some(&(code, a)), Some(&(_, b))) if a >= 2 && a > b => Some(code),
        _ => None,
    }
}

fn score(l: &Lang, text: &str) -> usize {
    let words = words_of(text);
    let hits = words
        .iter()
        .filter(|w| l.common.contains(&w.as_str()))
        .count();
    hits + text
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !l.letters.is_empty() && l.letters.contains(*c))
        .count()
        .min(3)
}

/// The code of a language given by name ("Hungarian") or code ("hu").
pub fn code_for(name_or_code: &str) -> Option<&'static str> {
    let t = name_or_code.trim();
    LANGS
        .iter()
        .find(|l| l.code.eq_ignore_ascii_case(t) || l.name.eq_ignore_ascii_case(t))
        .map(|l| l.code)
}

/// Best guess of a text's language among [`LANGS`] by common words, for the benchmark. `None` when no word
/// matches.
pub fn detect(text: &str) -> Option<&'static str> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut best: Option<(&'static str, usize)> = None;
    for l in LANGS {
        let mut hits = words
            .iter()
            .filter(|w| l.common.contains(&w.as_str()))
            .count();
        // Letters unique to a language weigh in too.
        hits += text
            .chars()
            .flat_map(char::to_lowercase)
            .filter(|c| !l.letters.is_empty() && l.letters.contains(*c))
            .count()
            .min(3);
        if hits > 0 && best.is_none_or(|(_, b)| hits > b) {
            best = Some((l.code, hits));
        }
    }
    best.map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn v(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn the_owners_screenshot_is_dropped_with_and_without_preferences() {
        let chip = "100% là một album rất hay";
        assert_eq!(filter_chip(chip, "Hát.", &[]), None);
        assert_eq!(filter_chip(chip, "Hát.", &v(&["hu", "en"])), None);
        // A Vietnamese writer keeps it.
        assert_eq!(
            filter_chip(chip, "Hát.", &v(&["vi"])).as_deref(),
            Some(chip)
        );
        assert_eq!(
            filter_chip(chip, "Tôi nghĩ là một album rất", &[]).as_deref(),
            Some(chip)
        );
    }

    #[test]
    fn preferences_cut_at_the_first_word_with_foreign_letters() {
        assert_eq!(
            filter_chip("hogy jövök, ça va", "Azt hiszem,", &v(&["hu"])).as_deref(),
            Some("hogy jövök,")
        );
        // Without preferences ordinary European letters are fine.
        assert_eq!(
            filter_chip("hogy jövök, ça va", "Azt hiszem,", &[]).as_deref(),
            Some("hogy jövök, ça va")
        );
        assert_eq!(
            filter_chip(" the meeting", "Let's move", &v(&["en"])).as_deref(),
            Some(" the meeting")
        );
        // German letters for a German writer, foreign for an English-only one.
        assert_eq!(
            filter_chip("für dich", "Das ist", &v(&["de"])).as_deref(),
            Some("für dich")
        );
        assert_eq!(filter_chip("für dich", "This is", &v(&["en"])), None);
    }

    #[test]
    fn common_words_catch_what_letters_cannot() {
        // French after Hungarian: "é" is a Hungarian letter too.
        assert_eq!(
            filter_chip(" la réforme du", "Bocs, de", &v(&["hu", "en"])),
            None
        );
        assert_eq!(
            filter_chip(" la révolution", "Bocs, de", &v(&["hu", "en"])),
            None
        );
        // The benchmark case: cut before "à", and the rest is still French.
        assert_eq!(
            filter_chip(" la pénurie à la crise", "Bocs, de", &v(&["hu", "en"])),
            None
        );
        assert_eq!(
            filter_chip(" la réforme du", "Bocs, de", &[]).as_deref(),
            Some(" la réforme du")
        );
        // The user's own languages are kept.
        assert_eq!(
            filter_chip(" de nem tudom", "Bocs,", &v(&["hu", "en"])).as_deref(),
            Some(" de nem tudom")
        );
        assert_eq!(
            filter_chip(" to the store", "I went", &v(&["hu", "en"])).as_deref(),
            Some(" to the store")
        );
        assert_eq!(
            filter_chip(" tomorrow", "See you", &v(&["hu", "en"])).as_deref(),
            Some(" tomorrow")
        );
    }

    #[test]
    fn strong_detection_needs_two_signals() {
        assert_eq!(
            detect_strong("Holnap nem tudok menni, mert beteg vagyok."),
            Some("hu")
        );
        assert_eq!(
            detect_strong("I can't go tomorrow because I am sick and that is it."),
            Some("en")
        );
        assert_eq!(detect_strong("OK"), None);
        assert_eq!(code_for("Hungarian"), Some("hu"));
        assert_eq!(code_for(" EN "), Some("en"));
        assert_eq!(code_for("Klingon"), None);
    }

    #[test]
    fn sanitizing_and_names() {
        assert_eq!(
            sanitize(&v(&["HU", "en", "xx", "hu", "de", "fr", "es"])),
            v(&["hu", "en", "de", "fr"])
        );
        assert_eq!(names(&v(&["hu", "en"])), "Hungarian and English");
        assert_eq!(
            names(&v(&["hu", "de", "en"])),
            "Hungarian, German and English"
        );
    }

    #[test]
    fn detects_the_obvious_cases() {
        assert_eq!(detect("Hát, szerintem ez nem jó."), Some("hu"));
        assert_eq!(detect("100% là một album rất hay"), Some("vi"));
        assert_eq!(
            detect("Thanks for the update, see you on Monday"),
            Some("en")
        );
        assert_eq!(detect("Ich bin nicht sicher, ob das geht."), Some("de"));
    }

    proptest! {
        /// The filter only ever shortens a chip at a word boundary; it never adds or changes characters.
        #[test]
        fn filter_is_a_prefix(chip in "\\PC{0,40}", before in "\\PC{0,20}", hu in any::<bool>()) {
            let prefs = if hu { v(&["hu"]) } else { vec![] };
            if let Some(out) = filter_chip(&chip, &before, &prefs) {
                prop_assert!(chip.starts_with(&out));
            }
        }
    }
}
