use crate::diagnostics;

#[derive(Clone)]
pub struct Failure<R> {
    pub message: String,
    pub retry: Option<R>,
}

impl<R> Failure<R> {
    pub fn log_error(self) -> Self {
        diagnostics::event(&format!("Error: {}", self.message));
        self
    }

    pub fn new(message: impl Into<String>, retry: R) -> Self {
        Self {
            message: message.into(),
            retry: Some(retry),
        }
    }
}

impl<R> From<String> for Failure<R> {
    fn from(message: String) -> Self {
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
