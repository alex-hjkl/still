//! Shared GPUI building blocks; screen-specific actions stay in the views.
use crate::theme::{BORDER, ERROR_BG, ERROR_TEXT, HOVER, MUTED, ON_ACCENT, PANEL, TEXT};
use gpui::{Context, IntoElement, Render, SharedString, Window, div, prelude::*, px, rgb, svg};

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
        .bg(rgb(HOVER))
        .text_color(rgb(TEXT))
        .cursor_pointer()
        .child(label.into())
}

pub fn icon_button(
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
        .hover(|view| view.bg(rgb(HOVER)))
        .child(content)
}

pub fn toolbar_icon(
    id: &'static str,
    glyph: &'static str,
    hint: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let hint = hint.into();
    icon_button(id, icon(glyph, MUTED))
        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint.clone())).into())
}

pub fn row_play_button(
    id: SharedString,
    hint: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let hint = hint.into();
    icon_button(id, icon_sized("play", MUTED, 16.))
        .size(px(28.))
        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint.clone())).into())
}

pub struct Hint(SharedString);

impl Hint {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(TEXT))
            .text_color(rgb(ON_ACCENT))
            .text_sm()
            .child(self.0.clone())
    }
}

pub fn track_row(id: usize, height: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .h(px(height))
        .flex()
        .items_center()
        .gap_4()
        .px_3()
        .border_b_1()
        .border_color(rgb(BORDER))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(PANEL)))
}

pub fn notification(message: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex_shrink_0()
        .p_3()
        .rounded_md()
        .bg(rgb(ERROR_BG))
        .text_color(rgb(ERROR_TEXT))
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
