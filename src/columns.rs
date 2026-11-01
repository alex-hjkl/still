//! Shared parent/current/preview layout for folders and the music library.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Column {
    Parent,
    Current,
    Preview,
}

pub(super) fn column_header(colors: Palette, title: impl Into<SharedString>) -> gpui::Div {
    div()
        .h(px(40.))
        .flex_shrink_0()
        .px_3()
        .flex()
        .items_center()
        .bg(rgb(colors.panel))
        .border_b_1()
        .border_color(rgb(colors.border))
        .child(
            div()
                .min_w_0()
                .font_weight(gpui::FontWeight::MEDIUM)
                .truncate()
                .child(title.into()),
        )
}

pub(super) fn column_message(colors: Palette, message: impl Into<SharedString>) -> gpui::Div {
    div()
        .p_3()
        .text_size(px(TEXT_SECONDARY))
        .text_color(rgb(colors.muted))
        .whitespace_normal()
        .child(message.into())
}

pub(super) fn column_layout(
    colors: Palette,
    parent: Option<gpui::AnyElement>,
    current: gpui::AnyElement,
    preview: gpui::AnyElement,
) -> gpui::Div {
    let has_parent = parent.is_some();
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .children(parent.map(|parent| {
            div()
                .w(relative(0.23))
                .h_full()
                .min_w_0()
                .flex_shrink_0()
                .flex()
                .border_r_1()
                .border_color(rgb(colors.border))
                .child(parent)
        }))
        .child(
            div()
                .w(relative(if has_parent { 0.42 } else { 0.58 }))
                .h_full()
                .min_w_0()
                .flex_shrink_0()
                .flex()
                .border_r_1()
                .border_color(rgb(colors.border))
                .child(current),
        )
        .child(preview)
}
