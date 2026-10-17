use super::*;

impl PlayerView {
    pub(super) fn restore_connection(&mut self, cx: &mut Context<Self>) {
        if self.connection.is_busy() || self.connection.client.is_some() {
            return;
        }
        self.error = None;
        self.connection.activity = Activity::Restoring;
        let revision = self.connection.revision;
        let task = CredentialStore::default().read(cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.connection.activity = Activity::Idle;
                match result {
                    Ok(Some(connection)) => {
                        this.connection.saved = true;
                        // Do not overwrite fields the user edited while Keychain was opening.
                        if this.connection.revision == revision {
                            this.server.update(cx, |field, cx| {
                                field.set_value(connection.server.clone(), cx)
                            });
                            this.username.update(cx, |field, cx| {
                                field.set_value(connection.username.clone(), cx)
                            });
                            this.password.update(cx, |field, cx| {
                                field.set_value(connection.password.clone(), cx)
                            });
                            this.connect_with(connection, false, cx);
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        this.connection.saved = true;
                        this.error = Some(Failure::new(
                            "Could not read Keychain. You can connect manually.",
                            RetryAction::RestoreConnection,
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn connect(&mut self, cx: &mut Context<Self>) {
        let connection = Connection {
            server: self.server.read(cx).value(),
            username: self.username.read(cx).value(),
            password: self.password.read(cx).value(),
        };
        self.connect_with(connection, true, cx);
    }

    pub(super) fn connect_with(
        &mut self,
        connection: Connection,
        persist: bool,
        cx: &mut Context<Self>,
    ) {
        if self.connection.is_busy() || self.connection.client.is_some() {
            return;
        }
        let client = Navidrome::new(
            &connection.server,
            &connection.username,
            &connection.password,
        );
        let client = match client {
            Ok(client) => client,
            Err(error) => {
                self.error = Some(format!("{error:#}").into());
                cx.notify();
                return;
            }
        };
        self.connection.activity = Activity::Connecting;
        self.error = None;
        self.browser.invalidate_requests();
        let request_id = self.browser.request_id();
        let remember = self.connection.remember;
        let had_saved = self.connection.saved;
        let task = cx.background_executor().spawn(async move {
            client.ping()?;
            let artists = client.artists()?;
            Ok::<_, anyhow::Error>((client, artists))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            // Save only after the server accepted the connection. A failed login
            // or an unavailable server must not replace the last working entry.
            let mut storage_result = Ok(());
            if result.is_ok() && persist {
                let task = this.update(cx, |this, cx| {
                    if !this.browser.is_current_request(request_id) { return None; }
                    let store = CredentialStore::default();
                    if remember { Some(store.write(&connection, cx)) }
                    else if had_saved { Some(store.delete(cx)) }
                    else { None }
                }).ok().flatten();
                if let Some(task) = task { storage_result = task.await; }
            }
            let _ = this.update(cx, |this, cx| {
                if !this.browser.is_current_request(request_id) {
                    return;
                }
                this.connection.activity = Activity::Idle;
                match result {
                    Ok((client, artists)) => {
                        this.connection.client = Some(client);
                        this.browser.apply(LibraryData::Artists(artists), false);
                        this.focus_library = true;
                        this.password.update(cx, |field, cx| field.clear(cx));
                        let section = this.settings.value().last_section;
                        if section != Section::Artists { this.open_section(section, cx); }
                        if persist {
                            if storage_result.is_ok() {
                                this.connection.saved = remember;
                            } else {
                                this.error = Some(if remember {
                                    Failure::new("Connected, but Keychain could not save your credentials.", RetryAction::SaveConnection(Arc::new(connection)))
                                } else {
                                    Failure::new("Connected, but the saved connection could not be removed from Keychain.", RetryAction::ForgetConnection)
                                });
                            }
                        }
                    }
                    Err(error) => this.error = Some(format!("{error:#}").into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn disconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connection.is_busy() {
            return;
        }
        self.remember_folder();
        self.pending_folder = None;
        self.browser.clear();
        self.reset_navigation_ui(cx);
        self.transport.disconnect();
        self.connection.disconnect();
        self.demo = false;
        self.error = None;
        self.likes = Likes::default();
        self.password.update(cx, |field, cx| field.clear(cx));
        self.forget_connection(cx);
        window.focus(&self.password.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn forget_connection(&mut self, cx: &mut Context<Self>) {
        if !self.connection.saved || self.connection.is_busy() {
            return;
        }
        self.connection.activity = Activity::Forgetting;
        self.error = None;
        let session_id = self.connection.session_id;
        let task = CredentialStore::default().delete(cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.connection.session_id != session_id {
                    return;
                }
                this.connection.activity = Activity::Idle;
                match result {
                    Ok(()) => this.connection.saved = false,
                    Err(_) => {
                        this.error = Some(Failure::new(
                            "Could not remove the connection from Keychain.",
                            RetryAction::ForgetConnection,
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn save_connection(&mut self, connection: Arc<Connection>, cx: &mut Context<Self>) {
        if self.connection.is_busy() {
            return;
        }
        self.connection.activity = Activity::Saving;
        let session_id = self.connection.session_id;
        let task = CredentialStore::default().write(&connection, cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.connection.session_id != session_id {
                    return;
                }
                this.connection.activity = Activity::Idle;
                match result {
                    Ok(()) => this.connection.saved = true,
                    Err(_) => {
                        this.error = Some(Failure::new(
                            "Keychain could not save your credentials.",
                            RetryAction::SaveConnection(connection),
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn connection_screen(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(380.))
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(44.))
                                    .bg(rgb(ACCENT))
                                    .rounded_xl()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("songs", ON_ACCENT)),
                            )
                            .child(
                                div()
                                    .text_size(px(32.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Still"),
                            ),
                    )
                    .child(
                        div()
                            .mt_3()
                            .text_size(px(26.))
                            .child("Your music. Nothing extra."),
                    )
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .line_height(px(22.))
                            .child("Connect to Navidrome to listen to your library."),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(MUTED)).child("Server URL"))
                            .child(self.server.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(MUTED)).child("Username"))
                            .child(self.username.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().text_color(rgb(MUTED)).child("Password"))
                            .child(self.password.clone()),
                    )
                    .child(
                        div()
                            .id("remember-connection")
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .text_sm()
                            .child(div().w(px(18.)).text_color(rgb(ACCENT)).child(
                                if self.connection.remember {
                                    "☑"
                                } else {
                                    "☐"
                                },
                            ))
                            .child("Remember connection")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.connection.is_busy() {
                                    this.connection.remember = !this.connection.remember;
                                    cx.notify();
                                }
                            })),
                    )
                    .when_some(self.error.clone(), |view, error| {
                        view.child(
                            div()
                                .text_sm()
                                .text_color(rgb(ERROR_TEXT))
                                .child(error.message),
                        )
                        .when(error.retry.is_some(), |view| {
                            view.child(
                                button("retry-connection", "Retry")
                                    .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                            )
                        })
                    })
                    .child(
                        button(
                            "connect",
                            match self.connection.activity {
                                Activity::Idle => "Connect",
                                Activity::Restoring => "Restoring connection…",
                                Activity::Connecting => "Connecting…",
                                Activity::Saving => "Saving connection…",
                                Activity::Forgetting => "Forgetting connection…",
                            },
                        )
                        .bg(rgb(ACCENT))
                        .text_color(rgb(ON_ACCENT))
                        .h(px(44.))
                        .hover(|s| s.bg(rgb(ACCENT_HOVER)))
                        .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("Your saved connection is protected by macOS Keychain."),
                    )
                    .when(self.connection.saved, |view| {
                        view.child(
                            button("forget-connection", "Forget connection")
                                .text_color(rgb(MUTED))
                                .on_click(cx.listener(|this, _, _, cx| this.forget_connection(cx))),
                        )
                    }),
            )
            .into_any_element()
    }
}
