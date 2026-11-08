//! Data sources for scorebug. Each provider turns a feed into normalized
//! [`GameSnapshot`]s for the [`scorebug_core::Tracker`]; nothing downstream
//! knows where the data came from.

pub mod espn;

use std::fmt;

use scorebug_core::GameSnapshot;

pub trait ScoreProvider {
    /// One poll: the current state of every game the provider tracks.
    fn poll(&mut self) -> Result<Vec<GameSnapshot>, ProviderError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError(pub String);

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProviderError {}

impl From<serde_json::Error> for ProviderError {
    fn from(e: serde_json::Error) -> Self {
        ProviderError(format!("bad JSON: {e}"))
    }
}

/// HTTP GET returning the body. A trait so tests can serve fixtures.
pub trait Fetch: Sync {
    fn fetch(&self, url: &str) -> Result<String, ProviderError>;
}

impl Fetch for ureq::Agent {
    fn fetch(&self, url: &str) -> Result<String, ProviderError> {
        self.get(url)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(32 << 20).read_to_string())
            .map_err(|e| ProviderError(format!("GET {url}: {e}")))
    }
}

/// An HTTP agent with sane timeouts for polling.
pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .user_agent("scorebug/0.1")
        .build()
        .into()
}
