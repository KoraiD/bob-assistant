//! Redaction of credential-like tokens before any prompt leaves the machine (GT-18, privacy steering).
//!
//! Matching is deliberately broad (privacy over recall): known key prefixes, JWTs, PEM blocks and long
//! opaque tokens. A redacted token becomes `[redacted]`. `scrub` is idempotent (property-tested).

pub const MARK: &str = "[redacted]";

const PREFIXES: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "pk_live_",
    "AKIA",
    "ASIA",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "AIza",
    "hf_",
];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '+' | '=' | '.')
}

/// True when `t` (a whitespace-free token) looks like a secret.
pub fn looks_secret(t: &str) -> bool {
    let t = t.trim_matches(|c: char| !is_token_char(c));
    if t.is_empty() || t == MARK {
        return false;
    }
    // Strip `label:` / `key=` wrappers so `token=ghp_...` is caught too.
    for sep in [':', '='] {
        if let Some((_, rest)) = t.split_once(sep) {
            if !rest.is_empty() && looks_secret(rest) {
                return true;
            }
        }
    }
    if PREFIXES
        .iter()
        .any(|p| t.starts_with(p) && t.len() >= p.len() + 8)
    {
        return true;
    }
    // JWT: three base64url parts, header starts with eyJ.
    if t.starts_with("eyJ") && t.matches('.').count() == 2 && t.len() >= 30 {
        return true;
    }
    if t.len() < 32 || !t.chars().all(is_token_char) {
        return false;
    }
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    let upper = t.chars().any(|c| c.is_ascii_uppercase());
    let lower = t.chars().any(|c| c.is_ascii_lowercase());
    let hex = t.chars().all(|c| c.is_ascii_hexdigit());
    // Long hex (hashes, API keys) or long mixed-case alphanumerics with digits. Plain long words or
    // URLs with dots and slashes but few digits are kept.
    (hex && digits > 0) || (upper && lower && digits >= 4 && !t.contains("//"))
}

/// Replace secrets in `text`. PEM blocks are replaced whole; other tokens one by one.
pub fn scrub(text: &str) -> String {
    let mut s = text.to_string();
    // PEM blocks (also partial ones, as typed text can stop mid-block).
    while let Some(start) = s.find("-----BEGIN ") {
        let end = s[start..]
            .find("-----END ")
            .and_then(|e| {
                s[start + e..]
                    .find("-----\n")
                    .or_else(|| s[start + e + 9..].find("-----").map(|x| x + 9))
                    .map(|x| start + e + x + 5)
            })
            .unwrap_or(s.len());
        s.replace_range(start..end.min(s.len()), MARK);
    }
    let mut out = String::with_capacity(s.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if looks_secret(token) {
            // Keep surrounding punctuation like quotes or a trailing comma.
            let lead: String = token.chars().take_while(|c| !is_token_char(*c)).collect();
            let trail: String = token
                .chars()
                .rev()
                .take_while(|c| !is_token_char(*c))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            out.push_str(&lead);
            out.push_str(MARK);
            out.push_str(&trail);
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in s.chars() {
        if c.is_whitespace() {
            flush(&mut token, &mut out);
            out.push(c);
        } else {
            token.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn fake(prefix: &str, n: usize) -> String {
        // Built at runtime so no key-shaped literal lives in the repository.
        format!("{prefix}{}", "a1B2".repeat(n / 4 + 1))
            .chars()
            .take(prefix.len() + n)
            .collect()
    }

    #[test]
    fn redacts_known_key_formats_and_keeps_context() {
        let key = fake("sk-", 40);
        assert_eq!(
            scrub(&format!("my key is {key}, ok")),
            "my key is [redacted], ok"
        );
        let gh = fake("ghp_", 36);
        assert_eq!(scrub(&format!("token={gh}")), "[redacted]");
        assert_eq!(
            scrub(&format!("\"{}\"", fake("AKIA", 16))),
            "\"[redacted]\""
        );
    }

    #[test]
    fn redacts_jwts_hashes_and_pem_blocks() {
        let jwt = format!(
            "eyJ{}.{}.{}",
            "a".repeat(20),
            "b".repeat(20),
            "c".repeat(20)
        );
        assert_eq!(scrub(&format!("Bearer {jwt}")), "Bearer [redacted]");
        let sha = "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597";
        assert_eq!(scrub(sha), "[redacted]");
        let pem = format!(
            "before\n-----BEGIN {k}-----\nMIIabc\n-----END {k}-----\nafter",
            k = "PRIVATE KEY"
        );
        assert_eq!(scrub(&pem), "before\n[redacted]\nafter");
        assert_eq!(
            scrub(&format!("-----BEGIN {} KEY-----\nMIIabc", "RSA PRIVATE")),
            "[redacted]"
        );
    }

    #[test]
    fn keeps_ordinary_prose_urls_and_long_words() {
        for text in [
            "Thanks for your email. I will get back to you soon.",
            "See https://github.com/KoraiD/bob-assistant/blob/main/README.md for details",
            "Antidisestablishmentarianism-and-floccinaucinihilipilification",
            "Meeting at 10:30 on 2026-11-23, room B-204",
            "sk-short",
        ] {
            assert_eq!(scrub(text), text, "{text}");
        }
    }

    proptest! {
        /// GT-18 / P5: the output never contains a credential-like token, and scrubbing is idempotent.
        #[test]
        fn scrub_removes_secrets_and_is_idempotent(
            words in proptest::collection::vec("[a-zA-Z]{1,10}", 0..12),
            secrets in proptest::collection::vec(prop_oneof![
                "sk-[a-zA-Z0-9]{20,40}", "ghp_[a-zA-Z0-9]{36}", "AKIA[A-Z0-9]{16}", "[0-9a-f]{40}",
            ], 0..4),
            seps in proptest::collection::vec(prop_oneof![Just(" "), Just("\n"), Just(", "), Just(" = ")], 0..16),
        ) {
            let mut parts: Vec<String> = words.clone();
            for (i, s) in secrets.iter().enumerate() { parts.insert(i.min(parts.len()), s.clone()); }
            let text: String = parts.iter().enumerate().map(|(i, p)| format!("{p}{}", seps.get(i).copied().unwrap_or(" "))).collect();
            let once = scrub(&text);
            for s in &secrets { prop_assert!(!once.contains(s.as_str()), "leaked {s} in {once}"); }
            for tok in once.split_whitespace() { prop_assert!(!looks_secret(tok), "secret-looking token {tok} survived"); }
            prop_assert_eq!(scrub(&once), once);
        }

        /// Text without any token-like character run of 32+ is unchanged (no false positives on prose).
        #[test]
        fn short_word_prose_is_untouched(words in proptest::collection::vec("[a-zA-Z]{1,12}[.,!?]?", 0..30)) {
            let text = words.join(" ");
            prop_assert_eq!(scrub(&text), text);
        }
    }
}
