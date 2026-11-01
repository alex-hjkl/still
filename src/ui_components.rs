//! Shared GPUI building blocks; screen-specific actions stay in the views.
use crate::theme::{Palette, TEXT_SECONDARY};
use gpui::{
    App, Context, Entity, EventEmitter, IntoElement, Render, SharedString, Subscription, Window,
    div, prelude::*, px, rgb, svg,
};

pub fn icon_sized(name: &'static str, color: u32, edge: f32) -> gpui::Svg {
    svg()
        .path(name)
        .size(px(edge))
        .flex_shrink_0()
        .text_color(rgb(color))
}

pub fn icon(name: &'static str, color: u32) -> gpui::Svg {
    icon_sized(name, color, 20.)
}

pub fn wordmark(size: f32) -> gpui::Div {
    div()
        .relative()
        // Optical center of "Still" in the bundled Source Sans 3 Medium.
        .top(px(size * 0.012))
        .text_size(px(size))
        .line_height(px(size))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child("Still")
}

pub fn button(
    colors: Palette,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .px_4()
        .py_2()
        .rounded_md()
        .bg(rgb(colors.hover))
        .text_color(rgb(colors.text))
        .cursor_pointer()
        .child(label.into())
}

pub fn icon_button(
    colors: Palette,
    id: impl Into<gpui::ElementId>,
    content: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(32.))
        .flex_shrink_0()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |view| view.bg(rgb(colors.hover)))
        .child(content)
}

pub fn toolbar_icon(
    colors: Palette,
    id: &'static str,
    glyph: &'static str,
    hint: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let hint = hint.into();
    icon_button(colors, id, icon(glyph, colors.muted))
        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint.clone())).into())
}

pub fn row_play_button(
    colors: Palette,
    id: SharedString,
    hint: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let hint = hint.into();
    icon_button(colors, id, icon_sized("play", colors.muted, 16.))
        .size(px(28.))
        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint.clone())).into())
}

pub struct Hint {
    text: Box<dyn Fn(&App) -> SharedString>,
    _subscription: Option<Subscription>,
}

impl Hint {
    pub fn new(text: impl Into<SharedString>) -> Self {
        let text = text.into();
        Self {
            text: Box::new(move |_| text.clone()),
            _subscription: None,
        }
    }

    pub fn live<T: EventEmitter<E>, E: 'static>(
        source: Entity<T>,
        cx: &mut Context<Self>,
        text: impl Fn(&T) -> SharedString + 'static,
    ) -> Self {
        let subscription = cx.subscribe(&source, |_, _, _: &E, cx| cx.notify());
        Self {
            text: Box::new(move |cx| text(source.read(cx))),
            _subscription: Some(subscription),
        }
    }
}

impl Render for Hint {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::colors(cx);
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(colors.tooltip_bg))
            .text_color(rgb(colors.tooltip_text))
            .text_size(px(TEXT_SECONDARY))
            .child((self.text)(cx))
    }
}

pub fn track_row(colors: Palette, id: usize, height: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .h(px(height))
        .flex()
        .items_center()
        .gap_4()
        .px_3()
        .border_b_1()
        .border_color(rgb(colors.row_border))
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(colors.panel)))
}

pub fn notification(colors: Palette, message: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex_shrink_0()
        .p_3()
        .rounded_md()
        .bg(rgb(colors.error_bg))
        .text_color(rgb(colors.error_text))
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_normal()
                .child(message.into()),
        )
}

pub fn count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

pub fn empty_state(colors: Palette, message: &str) -> gpui::AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .text_color(rgb(colors.muted))
        .child(
            svg()
                .path("songs")
                .size(px(32.))
                .text_color(rgb(colors.artwork_icon)),
        )
        .child(message.to_owned())
        .into_any_element()
}

pub fn toolbar_button(
    colors: Palette,
    id: &'static str,
    glyph: &'static str,
) -> gpui::Stateful<gpui::Div> {
    icon_button(colors, id, icon(glyph, colors.muted))
}
