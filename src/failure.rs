use crate::{api::Song, browser::LibraryRequest, credentials::Connection, diagnostics};
use std::sync::Arc;

// No Debug: a failed Keychain write may retain credentials until retry/dismissal.
#[derive(Clone)]
pub enum RetryAction {
    Library(LibraryRequest),
    Playback(u64),
    Like { song: Song, liked: bool },
    RestoreConnection,
    SaveConnection(Arc<Connection>),
    ForgetConnection,
}

#[derive(Clone)]
pub struct Failure {
    pub message: String,
    pub retry: Option<RetryAction>,
}

impl Failure {
    pub fn new(message: impl Into<String>, retry: RetryAction) -> Self {
        let message = message.into();
        diagnostics::event(&format!("Error: {message}"));
        Self {
            message,
            retry: Some(retry),
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        diagnostics::event(&format!("Error: {message}"));
        Self {
            message,
            retry: None,
        }
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
