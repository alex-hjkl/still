use crate::theme::{Palette, TEXT_SECONDARY};
use gpui::{Context, IntoElement, Render, SharedString, Window, div, prelude::*, px, rgb, svg};

pub(super) fn icon_sized(name: &'static str, color: u32, edge: f32) -> gpui::Svg {
    svg()
        .path(name)
        .size(px(edge))
        .flex_shrink_0()
        .text_color(rgb(color))
}

pub(super) fn icon(name: &'static str, color: u32) -> gpui::Svg {
    icon_sized(name, color, 20.)
}

pub(super) fn wordmark(size: f32) -> gpui::Div {
    div()
        .relative()
        // Optical center of "Still" in the bundled Source Sans 3 Medium.
        .top(px(size * 0.012))
        .text_size(px(size))
        .line_height(px(size))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child("Still")
}

pub(super) fn button(
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

pub(super) fn toolbar_button(
    colors: Palette,
    id: &'static str,
    glyph: &'static str,
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
        .child(icon(glyph, colors.muted))
}

pub(super) fn toolbar_icon(
    colors: Palette,
    id: &'static str,
    glyph: &'static str,
    hint: &'static str,
) -> gpui::Stateful<gpui::Div> {
    toolbar_button(colors, id, glyph).tooltip(move |_, cx| cx.new(|_| Hint::new(hint)).into())
}

pub(super) fn row_play_button(
    colors: Palette,
    id: SharedString,
    hint: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex_shrink_0()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |view| view.bg(rgb(colors.hover)))
        .child(icon_sized("play", colors.muted, 16.))
        .tooltip(move |_, cx| cx.new(|_| Hint::new(hint)).into())
}

pub(super) struct Hint(SharedString);

impl Hint {
    pub(super) fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
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
            .child(self.0.clone())
    }
}

pub(super) fn track_row(colors: Palette, id: usize, height: f32) -> gpui::Stateful<gpui::Div> {
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

pub(super) fn count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

pub(super) fn empty_state(colors: Palette, message: &str, query: &str) -> gpui::AnyElement {
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
        .child(if query.is_empty() {
            message.to_owned()
        } else {
            "No matches in the loaded list".into()
        })
        .into_any_element()
}
