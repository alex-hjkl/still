//! Explicit source of library data and permissions for local demo actions.
use crate::api::Navidrome;

// No Debug: the connected client contains authentication material.
#[derive(Clone, Default)]
pub enum Session {
    #[default]
    Disconnected,
    Demo,
    Connected(Navidrome),
}

impl Session {
    pub fn client(&self) -> Option<&Navidrome> {
        match self {
            Self::Connected(client) => Some(client),
            _ => None,
        }
    }
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Disconnected)
    }
    pub fn is_demo(&self) -> bool {
        matches!(self, Self::Demo)
    }
    pub fn library_id(&self) -> Option<String> {
        match self {
            Self::Disconnected => None,
            Self::Demo => Some("demo".into()),
            Self::Connected(client) => Some(client.library_id()),
        }
    }
    pub fn server_label(&self) -> String {
        match self {
            Self::Disconnected => String::new(),
            Self::Demo => "Demo · no playback".into(),
            Self::Connected(client) => client.server_label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnected_and_demo_are_distinct_even_though_neither_has_a_client() {
        let disconnected = Session::Disconnected;
        let demo = Session::Demo;
        assert!(disconnected.client().is_none());
        assert!(!disconnected.is_active());
        assert!(!disconnected.is_demo());
        assert!(disconnected.library_id().is_none());
        assert!(demo.client().is_none());
        assert!(demo.is_active());
        assert!(demo.is_demo());
        assert_eq!(demo.library_id().as_deref(), Some("demo"));
    }
    #[test]
    fn connected_sessions_keep_the_account_identity_and_client_when_cloned() {
        let client = Navidrome::new("https://fixture.invalid", "fixture", "fixture").unwrap();
        let id = client.library_id();
        let connected = Session::Connected(client);
        let cloned = connected.clone();
        assert!(cloned.is_active());
        assert!(!cloned.is_demo());
        assert!(cloned.client().is_some());
        assert_eq!(cloned.library_id().as_deref(), Some(id.as_str()));
        assert_ne!(cloned.library_id(), Session::Demo.library_id());
    }
}
