use crate::{api::Song, browser::LibraryRequest, diagnostics};

#[derive(Clone)]
pub enum RetryAction {
    Library(LibraryRequest),
    Playback(u64),
    Like { song: Song, liked: bool },
}

#[derive(Clone)]
pub struct Failure<R = RetryAction> {
    pub message: String,
    pub retry: Option<R>,
}

impl<R> Failure<R> {
    pub fn new(message: impl Into<String>, retry: R) -> Self {
        let message = message.into();
        diagnostics::event(&format!("Error: {message}"));
        Self {
            message,
            retry: Some(retry),
        }
    }
}

impl<R> From<String> for Failure<R> {
    fn from(message: String) -> Self {
        diagnostics::event(&format!("Error: {message}"));
        Self {
            message,
            retry: None,
        }
    }
}

impl<R> From<&str> for Failure<R> {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum Operation {
    Library,
    Playback,
    Like(String),
}

#[derive(Default)]
pub struct Failures(Vec<(Operation, Failure)>);

impl Failures {
    pub fn set(&mut self, operation: Operation, failure: Failure) {
        self.clear(&operation);
        self.0.push((operation, failure));
    }

    pub fn clear(&mut self, operation: &Operation) {
        self.0.retain(|(candidate, _)| candidate != operation);
    }

    pub fn take(&mut self, operation: &Operation) -> Option<Failure> {
        let index = self
            .0
            .iter()
            .position(|(candidate, _)| candidate == operation)?;
        Some(self.0.remove(index).1)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Operation, Failure)> {
        self.0.iter()
    }
}
