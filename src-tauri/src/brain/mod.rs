//! The Brain layer: "Bring Your Own Brain" (spec `.kiro/specs/brain-router`).
//!
//! A [`BrainConfig`] describes one model endpoint (built-in sidecar, Ollama, LM Studio or any
//! OpenAI-compatible server). [`client::HttpBrain`] talks to it and streams tokens. Capability flags
//! decide which BOB feature may use which brain (BR-1, BR-4).

pub mod client;
pub mod dialect;
pub mod models;
pub mod registry;
pub mod secrets;
pub mod service;
pub mod sidecar;
pub mod sse;
#[cfg(test)]
pub(crate) mod testserver;

use serde::{Deserialize, Serialize};
use url::Url;

pub use dialect::{ChatRequest, CompletionRequest, Dialect};

/// What a brain can be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Capabilities {
    /// Supports raw continuation (`/completion` or `/v1/completions`): required for Ghost Text (BR-4).
    pub ghost: bool,
    /// Supports chat instructions: Selection Actions and Dictation cleanup.
    pub chat: bool,
}

/// Which kind of server a brain is. Drives defaults and the dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// BOB's own llama-server sidecar (managed by [`sidecar`]).
    Builtin,
    Ollama,
    LmStudio,
    /// Any OpenAI-compatible endpoint, local or cloud.
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn dialect(self) -> Dialect {
        match self {
            ProviderKind::Builtin => Dialect::LlamaCpp,
            _ => Dialect::OpenAi,
        }
    }

    /// Suggested base URL for a new brain of this kind.
    pub fn default_endpoint(self) -> &'static str {
        match self {
            ProviderKind::Builtin => "http://127.0.0.1:0",
            ProviderKind::Ollama => "http://127.0.0.1:11434",
            ProviderKind::LmStudio => "http://127.0.0.1:1234",
            ProviderKind::OpenAiCompatible => "https://api.openai.com",
        }
    }

    /// Default capabilities. Cloud chat APIs rarely offer raw completions, so Ghost is off for them
    /// until a connection test proves otherwise.
    pub fn default_capabilities(self) -> Capabilities {
        match self {
            ProviderKind::Builtin | ProviderKind::Ollama | ProviderKind::LmStudio => Capabilities {
                ghost: true,
                chat: true,
            },
            ProviderKind::OpenAiCompatible => Capabilities {
                ghost: false,
                chat: true,
            },
        }
    }
}

/// A configured brain. Persisted in `brains.json`; the API key itself lives in the Keychain (BR-6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrainConfig {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    /// Model name sent to OpenAI-compatible servers (optional for single-model servers).
    #[serde(default)]
    pub model: Option<String>,
    pub capabilities: Capabilities,
    /// True when an API key is stored in the Keychain for this brain.
    #[serde(default)]
    pub has_api_key: bool,
}

impl BrainConfig {
    /// True when the endpoint is not on this machine, so text leaves the device (BR-7).
    pub fn is_cloud(&self) -> bool {
        is_cloud_endpoint(&self.endpoint)
    }

    pub fn dialect(&self) -> Dialect {
        self.kind.dialect()
    }
}

/// Loopback hosts (`localhost`, `127.0.0.0/8`, `::1`) are local; everything else is cloud.
/// Unparseable endpoints count as cloud, the safe default for labeling.
pub fn is_cloud_endpoint(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint) else {
        return true;
    };
    match url.host() {
        Some(url::Host::Domain(d)) => !d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => !ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => !ip.is_loopback(),
        None => true,
    }
}

/// Errors from talking to a brain. Messages are user-facing (shown in the Brain panel).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BrainError {
    #[error("this brain cannot be used for {0}")]
    Unsupported(&'static str),
    #[error("invalid endpoint URL: {0}")]
    InvalidEndpoint(String),
    #[error("cannot reach the brain: {0}")]
    Unreachable(String),
    #[error("the brain rejected the API key (HTTP {0})")]
    Unauthorized(u16),
    #[error("the brain answered with HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("the brain sent a malformed stream: {0}")]
    Protocol(String),
    #[error("the brain reported an error: {0}")]
    Server(String),
    #[error("timed out waiting for the brain")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn loopback_endpoints_are_local_everything_else_is_cloud() {
        for local in [
            "http://127.0.0.1:11434",
            "http://localhost:1234",
            "http://[::1]:8080",
            "http://127.8.9.1",
        ] {
            assert!(!is_cloud_endpoint(local), "{local}");
        }
        for cloud in [
            "https://api.openai.com",
            "http://192.168.1.20:11434",
            "http://localhost.evil.com",
            "http://10.0.0.1",
            "not a url",
            "",
        ] {
            assert!(is_cloud_endpoint(cloud), "{cloud}");
        }
    }

    #[test]
    fn provider_defaults_are_consistent() {
        assert_eq!(ProviderKind::Builtin.dialect(), Dialect::LlamaCpp);
        assert_eq!(ProviderKind::Ollama.dialect(), Dialect::OpenAi);
        assert!(!ProviderKind::OpenAiCompatible.default_capabilities().ghost);
        assert!(ProviderKind::Ollama.default_capabilities().ghost);
        for k in [ProviderKind::Ollama, ProviderKind::LmStudio] {
            assert!(!is_cloud_endpoint(k.default_endpoint()));
        }
    }

    proptest! {
        /// Any non-loopback IPv4 host is labeled cloud (BR-7: never under-report data leaving the Mac).
        #[test]
        fn non_loopback_ipv4_is_cloud(a in 0u8..=255, b in any::<u8>(), c in any::<u8>(), d in any::<u8>(), port in 1u16..) {
            let url = format!("http://{a}.{b}.{c}.{d}:{port}");
            prop_assert_eq!(is_cloud_endpoint(&url), a != 127);
        }
    }
}
