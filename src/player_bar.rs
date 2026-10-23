use super::*;

impl PlayerView {
    pub(super) fn player_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::colors(cx);
        let current = self.transport.queue().current();
        let enabled = current.is_some();
        let duration = current.map_or(0, |song| song.duration);
        let position = self
            .seek_slider
            .read(cx)
            .preview()
            .map_or(self.transport.state().position.as_secs(), |ratio| {
                (duration as f32 * ratio).round() as u64
            });

        let progress = if duration > 0 {
            self.transport.state().position.as_secs_f32() / duration as f32
        } else {
            0.
        };
        self.seek_slider.update(cx, |slider, cx| {
            slider.configure(progress, enabled && duration > 0, Some(duration), cx)
        });
        self.volume_slider.update(cx, |slider, cx| {
            slider.configure(self.settings.value().volume, true, None, cx)
        });
        div()
            .h(px(PLAYER_HEIGHT))
            .flex_shrink_0()
            .bg(rgb(colors.panel))
            .border_t_1()
            .border_color(rgb(colors.border))
            .flex()
            .flex_col()
            .justify_center()
            .px_4()
            .child(
                div()
                    .h(px(60.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_5()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_3()
                            .min_w_0()
                            .child(artwork(
                                colors,
                                self.connection.read(cx).client.as_ref(),
                                current.and_then(|song| song.cover_art.as_deref()),
                                40.,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .truncate()
                                            .child(
                                                current
                                                    .map_or("Choose some music".into(), |song| {
                                                        song.title.clone()
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(TEXT_SECONDARY))
                                            .text_color(rgb(colors.muted))
                                            .truncate()
                                            .child(if self.transport.state().is_loading() {
                                                "Loading audio…".into()
                                            } else {
                                                current.map_or(
                                                    "Your library is ready".into(),
                                                    |song| song.artist.clone(),
                                                )
                                            }),
                                    ),
                            )
                            .when_some(current, |view, song| {
                                view.child(self.like_button(song, "player-like", cx))
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("shuffle")
                                    .size(px(30.))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .when(self.transport.queue().is_shuffled(), |view| {
                                        view.bg(rgb(colors.selected))
                                    })
                                    .hover(move |view| view.bg(rgb(colors.hover)))
                                    .child(icon(
                                        "shuffle",
                                        if self.transport.queue().is_shuffled() {
                                            colors.accent
                                        } else {
                                            colors.muted
                                        },
                                    ))
                                    .tooltip({
                                        let shuffled = self.transport.queue().is_shuffled();
                                        move |_, cx| {
                                            cx.new(|_| {
                                                Hint::new(if shuffled {
                                                    "Turn shuffle off · s"
                                                } else {
                                                    "Turn shuffle on · s"
                                                })
                                            })
                                            .into()
                                        }
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_shuffle(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("previous")
                                    .size(px(32.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(icon(
                                        "previous",
                                        if enabled { colors.text } else { colors.muted },
                                    ))
                                    .tooltip(|_, cx| {
                                        cx.new(|_| Hint::new("Previous song or restart · [")).into()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| this.previous(cx))),
                            )
                            .child(
                                div()
                                    .id("toggle")
                                    .size(px(38.))
                                    .rounded_full()
                                    .bg(rgb(if enabled {
                                        colors.accent
                                    } else {
                                        colors.border
                                    }))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(icon_sized(
                                        if self.transport.state().is_playing() {
                                            "pause"
                                        } else {
                                            "play"
                                        },
                                        if enabled {
                                            colors.on_accent
                                        } else {
                                            colors.muted
                                        },
                                        20.,
                                    ))
                                    .tooltip({
                                        let playing = self.transport.state().is_playing();
                                        move |_, cx| {
                                            cx.new(|_| {
                                                Hint::new(if !enabled {
                                                    "Choose a song to play"
                                                } else if playing {
                                                    "Pause · Space"
                                                } else {
                                                    "Play · Space"
                                                })
                                            })
                                            .into()
                                        }
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
                            )
                            .child(
                                div()
                                    .id("next")
                                    .size(px(32.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(icon(
                                        "next",
                                        if enabled { colors.text } else { colors.muted },
                                    ))
                                    .tooltip(|_, cx| cx.new(|_| Hint::new("Next song · ]")).into())
                                    .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("mute")
                                    .size(px(30.))
                                    .rounded_full()
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(move |view| view.bg(rgb(colors.hover)))
                                    .child(icon(
                                        if self.settings.value().volume == 0. {
                                            "volume-muted"
                                        } else {
                                            "volume"
                                        },
                                        colors.muted,
                                    ))
                                    .tooltip({
                                        let muted = self.settings.value().volume == 0.;
                                        move |_, cx| {
                                            cx.new(|_| {
                                                Hint::new(if muted { "Unmute" } else { "Mute" })
                                            })
                                            .into()
                                        }
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_mute(cx))),
                            )
                            .child(div().w(px(76.)).child(self.volume_slider.clone()))
                            .child(
                                div()
                                    .w(px(36.))
                                    .text_size(px(TEXT_CAPTION))
                                    .text_color(rgb(colors.muted))
                                    .child(format!(
                                        "{}%",
                                        (self.settings.value().volume * 100.).round() as u32
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .child(
                        div()
                            .w(px(34.))
                            .text_size(px(TEXT_CAPTION))
                            .text_color(rgb(colors.muted))
                            .child(time_label(position)),
                    )
                    .child(div().flex_1().min_w_0().child(self.seek_slider.clone()))
                    .child(
                        div()
                            .w(px(34.))
                            .text_size(px(TEXT_CAPTION))
                            .text_right()
                            .text_color(rgb(colors.muted))
                            .child(time_label(duration)),
                    ),
            )
            .into_any_element()
    }
}
