//! Owns authentication, credential persistence and their failures independently of navigation.
use crate::{
    api::Navidrome,
    credentials::{Connection, CredentialStore},
    failure::Failure,
};
use anyhow::Result;
use gpui::{Context, EventEmitter, Task};
use std::sync::Arc;

#[derive(Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Idle,
    Restoring,
    Connecting,
    Saving,
    Forgetting,
}

// No Debug: saving may retain credentials until retry or explicit dismissal.
#[derive(Clone)]
pub enum ConnectionRetry {
    Restore,
    Save(Arc<Connection>),
    Forget,
}

pub enum ConnectionEvent {
    Changed,
    Restored(Arc<Connection>),
    Connected,
}

struct Persistence {
    connection: Arc<Connection>,
    remember: bool,
    result: Result<()>,
}

pub struct ConnectionController {
    pub client: Option<Navidrome>,
    pub remember: bool,
    pub saved: bool,
    pub session_id: u64,
    pub activity: Activity,
    revision: u64,
    operation: u64,
    failure: Option<Failure<ConnectionRetry>>,
    task: Option<Task<()>>,
    #[cfg(feature = "ui-tests")]
    test_store_available: Option<bool>,
}

impl EventEmitter<ConnectionEvent> for ConnectionController {}

impl ConnectionController {
    pub fn new(from_environment: bool) -> Self {
        Self {
            client: None,
            remember: !from_environment,
            saved: false,
            session_id: 0,
            activity: Activity::Idle,
            revision: 0,
            operation: 0,
            failure: None,
            task: None,
            #[cfg(feature = "ui-tests")]
            test_store_available: None,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.activity != Activity::Idle
    }

    pub fn toggle_remember(&mut self, cx: &mut Context<Self>) {
        if !self.is_busy() {
            self.remember = !self.remember;
            cx.emit(ConnectionEvent::Changed);
        }
    }

    pub fn edited(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn failure(&self) -> Option<&Failure<ConnectionRetry>> {
        self.failure.as_ref()
    }

    pub fn dismiss_failure(&mut self) {
        self.failure = None;
    }

    fn begin(&mut self, activity: Activity) -> u64 {
        self.task = None;
        self.operation = self.operation.wrapping_add(1);
        self.activity = activity;
        self.failure = None;
        self.operation
    }

    fn is_current(&self, operation: u64) -> bool {
        self.operation == operation && self.is_busy()
    }

    pub fn restore(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() || self.client.is_some() {
            return;
        }
        let operation = self.begin(Activity::Restoring);
        let revision = self.revision;
        let task = CredentialStore::default().read(cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if !this.is_current(operation) {
                    return;
                }
                if let Some(connection) = this.finish_restore(operation, revision, result) {
                    cx.emit(ConnectionEvent::Restored(connection.clone()));
                    this.connect(connection, false, cx);
                }
                cx.emit(ConnectionEvent::Changed);
            });
        }));
        cx.emit(ConnectionEvent::Changed);
    }

    fn finish_restore(
        &mut self,
        operation: u64,
        revision: u64,
        result: Result<Option<Connection>>,
    ) -> Option<Arc<Connection>> {
        if !self.is_current(operation) {
            return None;
        }
        self.activity = Activity::Idle;
        match result {
            Ok(Some(connection)) => {
                self.saved = true;
                // A slow credential store must not overwrite edits made in the meantime.
                (self.revision == revision).then(|| Arc::new(connection))
            }
            Ok(None) => None,
            Err(_) => {
                self.saved = true;
                self.failure = Some(Failure::new(
                    "Could not read the saved connection. You can connect manually.",
                    ConnectionRetry::Restore,
                ));
                None
            }
        }
    }

    pub fn connect(&mut self, connection: Arc<Connection>, persist: bool, cx: &mut Context<Self>) {
        if self.is_busy() || self.client.is_some() {
            return;
        }
        let operation = self.begin(Activity::Connecting);
        let remember = self.remember;
        let had_saved = self.saved;
        let credentials = connection.clone();
        let task = cx
            .background_executor()
            .spawn(async move { authenticate(&credentials) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            // Never replace a working saved login until authentication succeeds.
            let storage = if result.is_ok() && persist {
                this.update(cx, |this, cx| {
                    if !this.is_current(operation) {
                        return None;
                    }
                    let store = CredentialStore::default();
                    if remember {
                        Some(store.write(&connection, cx))
                    } else if had_saved {
                        Some(store.delete(cx))
                    } else {
                        None
                    }
                })
                .ok()
                .flatten()
            } else {
                None
            };
            let storage_result = match storage {
                Some(task) => task.await,
                None => Ok(()),
            };
            let persistence = persist.then_some(Persistence {
                connection,
                remember,
                result: storage_result,
            });
            let _ = this.update(cx, |this, cx| {
                if !this.is_current(operation) {
                    return;
                }
                if this.finish_connect(operation, result, persistence) {
                    cx.emit(ConnectionEvent::Connected);
                }
                cx.emit(ConnectionEvent::Changed);
            });
        }));
        cx.emit(ConnectionEvent::Changed);
    }

    fn finish_connect(
        &mut self,
        operation: u64,
        result: Result<Navidrome>,
        persistence: Option<Persistence>,
    ) -> bool {
        if !self.is_current(operation) {
            return false;
        }
        self.activity = Activity::Idle;
        match result {
            Ok(client) => {
                self.client = Some(client);
                if let Some(persistence) = persistence {
                    self.finish_persistence(
                        persistence.remember,
                        persistence.result,
                        if persistence.remember {
                            ConnectionRetry::Save(persistence.connection)
                        } else {
                            ConnectionRetry::Forget
                        },
                    );
                }
                true
            }
            Err(error) => {
                self.failure = Some(format!("{error:#}").into());
                false
            }
        }
    }

    fn finish_persistence(&mut self, saved: bool, result: Result<()>, retry: ConnectionRetry) {
        match result {
            Ok(()) => {
                self.saved = saved;
                self.failure = None;
            }
            Err(_) => {
                self.failure = Some(Failure::new(
                    if saved {
                        "Could not save the connection in the credential store."
                    } else {
                        "Could not remove the saved connection from the credential store."
                    },
                    retry,
                ));
            }
        }
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(retry) = self
            .failure
            .as_ref()
            .and_then(|failure| failure.retry.clone())
        else {
            return;
        };
        match retry {
            ConnectionRetry::Restore => self.restore(cx),
            retry => self.persist(retry, cx),
        }
    }

    pub fn forget(&mut self, cx: &mut Context<Self>) {
        if self.saved && !self.is_busy() {
            self.persist(ConnectionRetry::Forget, cx);
        }
    }

    fn persist(&mut self, retry: ConnectionRetry, cx: &mut Context<Self>) {
        let (activity, saved) = match &retry {
            ConnectionRetry::Save(_) => (Activity::Saving, true),
            ConnectionRetry::Forget => (Activity::Forgetting, false),
            ConnectionRetry::Restore => return,
        };
        let task = self.credential_task(&retry, cx);
        let operation = self.begin(activity);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if !this.is_current(operation) {
                    return;
                }
                this.activity = Activity::Idle;
                this.finish_persistence(saved, result, retry);
                cx.emit(ConnectionEvent::Changed);
            });
        }));
        cx.emit(ConnectionEvent::Changed);
    }

    fn credential_task(&self, retry: &ConnectionRetry, cx: &gpui::App) -> Task<Result<()>> {
        #[cfg(feature = "ui-tests")]
        if let Some(available) = self.test_store_available {
            return Task::ready(if available {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Fixture credential store unavailable"))
            });
        }
        let store = CredentialStore::default();
        match retry {
            ConnectionRetry::Save(connection) => store.write(connection, cx),
            ConnectionRetry::Forget => store.delete(cx),
            ConnectionRetry::Restore => unreachable!("restoring does not write credentials"),
        }
    }

    #[cfg(feature = "ui-tests")]
    pub fn test_save_failure(&mut self, cx: &mut Context<Self>) {
        self.test_store_available = Some(false);
        self.persist(
            ConnectionRetry::Save(Arc::new(Connection {
                server: "https://fixture.invalid".into(),
                username: "fixture".into(),
                password: "fixture".into(),
            })),
            cx,
        );
    }

    #[cfg(feature = "ui-tests")]
    pub fn test_store_available(&mut self) {
        self.test_store_available = Some(true);
    }

    pub fn disconnect(&mut self, cx: &mut Context<Self>) -> bool {
        if self.is_busy() {
            return false;
        }
        self.client = None;
        self.session_id = self.session_id.wrapping_add(1);
        self.operation = self.operation.wrapping_add(1);
        self.task = None;
        self.failure = None;
        self.forget(cx);
        cx.emit(ConnectionEvent::Changed);
        true
    }
}

fn authenticate(connection: &Connection) -> Result<Navidrome> {
    let client = Navidrome::new(
        &connection.server,
        &connection.username,
        &connection.password,
    )?;
    client.ping()?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    fn credentials() -> Connection {
        Connection {
            server: "https://music.example".into(),
            username: "fixture".into(),
            password: "fixture-secret".into(),
        }
    }

    fn client() -> Navidrome {
        let connection = credentials();
        Navidrome::new(
            &connection.server,
            &connection.username,
            &connection.password,
        )
        .unwrap()
    }

    #[test]
    fn authentication_does_not_require_or_load_the_artists_section() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(request.starts_with("GET /rest/ping.view?"));
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
            }
            let body = r#"{"subsonic-response":{"status":"ok"}}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let result = authenticate(&Connection {
            server: format!("http://{address}"),
            ..credentials()
        });
        server.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn restore_preserves_edits_and_rejects_a_late_completion() {
        let mut controller = ConnectionController::new(false);
        let first = controller.begin(Activity::Restoring);
        controller.edited();
        assert!(
            controller
                .finish_restore(first, 0, Ok(Some(credentials())))
                .is_none()
        );
        assert!(controller.saved);
        assert!(!controller.is_busy());

        let second = controller.begin(Activity::Restoring);
        assert!(
            controller
                .finish_restore(first, 0, Err(anyhow::anyhow!("late")))
                .is_none()
        );
        assert!(controller.is_current(second));
        assert!(controller.failure().is_none());
        assert!(
            controller
                .finish_restore(second, controller.revision, Ok(Some(credentials())))
                .is_some()
        );
    }

    #[test]
    fn credential_failure_keeps_the_connected_client_and_secret_until_retry_or_dismissal() {
        let mut controller = ConnectionController::new(false);
        let credentials = Arc::new(credentials());
        let operation = controller.begin(Activity::Connecting);
        assert!(controller.finish_connect(
            operation,
            Ok(client()),
            Some(Persistence {
                connection: credentials.clone(),
                remember: true,
                result: Err(anyhow::anyhow!("store unavailable")),
            })
        ));
        assert!(controller.client.is_some());
        assert!(!controller.saved);
        assert!(!controller.is_busy());
        let Some(ConnectionRetry::Save(retry)) = controller
            .failure()
            .and_then(|failure| failure.retry.as_ref())
        else {
            panic!("saving the connection must remain retryable");
        };
        assert!(Arc::ptr_eq(retry, &credentials));
        assert_eq!(retry.password, "fixture-secret");
        controller.finish_persistence(true, Ok(()), ConnectionRetry::Save(credentials.clone()));
        assert!(controller.saved);
        assert!(controller.failure().is_none());
        assert_eq!(Arc::strong_count(&credentials), 1);

        controller.finish_persistence(
            true,
            Err(anyhow::anyhow!("store unavailable")),
            ConnectionRetry::Save(credentials.clone()),
        );
        assert_eq!(Arc::strong_count(&credentials), 2);
        controller.dismiss_failure();
        assert_eq!(Arc::strong_count(&credentials), 1);
    }

    #[test]
    fn failed_login_preserves_the_previous_saved_connection_and_stale_results_do_not_connect() {
        let mut controller = ConnectionController::new(false);
        controller.saved = true;
        let first = controller.begin(Activity::Connecting);
        let second = controller.begin(Activity::Connecting);
        assert!(!controller.finish_connect(first, Ok(client()), None));
        assert!(controller.client.is_none());
        assert!(controller.is_current(second));
        assert!(!controller.finish_connect(
            second,
            Err(anyhow::anyhow!("invalid login")),
            Some(Persistence {
                connection: Arc::new(credentials()),
                remember: false,
                result: Ok(()),
            })
        ));
        assert!(controller.saved);
        assert!(controller.client.is_none());
        assert!(controller.failure().is_some());
    }

    #[test]
    fn failed_forget_remains_retryable_without_a_connected_client() {
        let mut controller = ConnectionController::new(false);
        controller.saved = true;
        controller.finish_persistence(
            false,
            Err(anyhow::anyhow!("store unavailable")),
            ConnectionRetry::Forget,
        );
        assert!(controller.saved);
        assert!(matches!(
            controller
                .failure()
                .and_then(|failure| failure.retry.as_ref()),
            Some(ConnectionRetry::Forget)
        ));
        controller.finish_persistence(false, Ok(()), ConnectionRetry::Forget);
        assert!(!controller.saved);
        assert!(controller.failure().is_none());
    }
}
