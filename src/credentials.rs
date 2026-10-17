use anyhow::{Context as _, Result};
use gpui::{App, Task};
use serde::{Deserialize, Serialize};

// A single app-owned item avoids mixing a password with another server/account.
const KEYCHAIN_ITEM: &str = "com.still.player.navidrome";

// No Debug: the payload contains the password, not just connection metadata.
#[derive(Serialize, Deserialize)]
pub struct Connection {
    pub server: String,
    pub username: String,
    pub password: String,
}

impl Connection {
    fn decode(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).context("Could not read the saved connection")
    }
}

pub struct CredentialStore {
    key: &'static str,
}

impl Default for CredentialStore {
    fn default() -> Self {
        Self::new(KEYCHAIN_ITEM)
    }
}

impl CredentialStore {
    pub fn new(key: &'static str) -> Self {
        Self { key }
    }

    pub fn read(&self, cx: &App) -> Task<Result<Option<Connection>>> {
        let task = cx.read_credentials(self.key);
        cx.background_executor().spawn(async move {
            task.await?
                .map(|(_, bytes)| Connection::decode(&bytes))
                .transpose()
        })
    }

    pub fn write(&self, connection: &Connection, cx: &App) -> Task<Result<()>> {
        // The serialized payload goes straight to Keychain, never to a file/log.
        let bytes = serde_json::to_vec(connection).expect("connection contains only strings");
        cx.write_credentials(self.key, &connection.username, &bytes)
    }

    pub fn delete(&self, cx: &App) -> Task<Result<()>> {
        cx.delete_credentials(self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_round_trip_keeps_server_account_and_secret_together() {
        let connection = Connection {
            server: "https://music.example/прокси/".into(),
            username: "a+b".into(),
            password: "пароль\n\"🔑".into(),
        };
        let bytes = serde_json::to_vec(&connection).unwrap();
        let restored = Connection::decode(&bytes).unwrap();
        assert_eq!(restored.server, connection.server);
        assert_eq!(restored.username, connection.username);
        assert_eq!(restored.password, connection.password);
        assert!(Connection::decode(br#"{"server":"https://example.com"}"#).is_err());
    }
}
