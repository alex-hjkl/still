//! A reusable slider. Pointer state belongs to the control, not the application view.
use crate::api::time_label;
use gpui::{
    Context, DispatchPhase, EventEmitter, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, Window, div, prelude::*, px, relative, rgb,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommitMode {
    OnRelease,
    Continuous,
}

pub enum SliderEvent {
    Preview,
    Change(f32),
}

#[derive(Default)]
struct Interaction {
    drag: Option<f32>,
    hover: Option<f32>,
}

impl Interaction {
    fn begin(&mut self, value: f32) {
        self.drag = Some(value.clamp(0., 1.));
    }
    fn motion(&mut self, value: f32, held: bool) {
        if self.drag.is_some() {
            self.drag = held.then(|| value.clamp(0., 1.));
        }
    }
    fn finish(&mut self, value: f32) -> Option<f32> {
        self.drag.take().map(|_| value.clamp(0., 1.))
    }
}

pub struct Slider {
    id: &'static str,
    mode: CommitMode,
    value: f32,
    enabled: bool,
    duration: Option<u64>,
    pointer: Interaction,
}
impl EventEmitter<SliderEvent> for Slider {}

impl Slider {
    pub fn new(id: &'static str, mode: CommitMode) -> Self {
        Self {
            id,
            mode,
            value: 0.,
            enabled: false,
            duration: None,
            pointer: Interaction::default(),
        }
    }
    pub fn configure(
        &mut self,
        value: f32,
        enabled: bool,
        duration: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let value = value.clamp(0., 1.);
        if (self.value, self.enabled, self.duration) != (value, enabled, duration) {
            self.value = value;
            self.enabled = enabled;
            self.duration = duration;
            if !enabled {
                self.cancel(cx);
            }
            cx.notify();
        }
    }
    pub fn preview(&self) -> Option<f32> {
        self.pointer.drag
    }
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.pointer.drag.is_some() || self.pointer.hover.is_some() {
            self.pointer = Interaction::default();
            cx.emit(SliderEvent::Preview);
            cx.notify();
        }
    }
    fn begin(&mut self, ratio: f32, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        self.pointer.begin(ratio);
        if self.mode == CommitMode::Continuous {
            cx.emit(SliderEvent::Change(ratio));
        }
        cx.emit(SliderEvent::Preview);
        cx.notify();
    }
    fn motion(&mut self, ratio: f32, inside: bool, held: bool, cx: &mut Context<Self>) {
        let before = (self.pointer.drag, self.pointer.hover);
        self.pointer.motion(ratio, held);
        self.pointer.hover = (self.enabled && inside).then_some(ratio);
        if self.pointer.drag.is_some() && self.mode == CommitMode::Continuous {
            cx.emit(SliderEvent::Change(ratio));
        }
        if before != (self.pointer.drag, self.pointer.hover) {
            cx.emit(SliderEvent::Preview);
            cx.notify();
        }
    }
    fn finish(&mut self, ratio: f32, cx: &mut Context<Self>) {
        if let Some(value) = self.pointer.finish(ratio) {
            cx.emit(SliderEvent::Change(value));
            cx.emit(SliderEvent::Preview);
            cx.notify();
        }
    }
}

impl Render for Slider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::colors(cx);
        let value = self.pointer.drag.unwrap_or(self.value);
        let hint = self.pointer.drag.or(self.pointer.hover);
        let thumb = self.enabled && (self.mode == CommitMode::Continuous || hint.is_some());
        let duration = self.duration;
        let view = cx.weak_entity();
        let enabled = self.enabled;
        div()
            .id(self.id)
            .relative()
            .h(px(18.))
            .w_full()
            .when(enabled, |view| view.cursor_pointer())
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let track = gpui::Bounds::new(
                            gpui::point(bounds.left(), bounds.top() + px(7.)),
                            gpui::size(bounds.size.width, px(3.)),
                        );
                        window.paint_quad(gpui::fill(track, rgb(colors.border)));
                        window.paint_quad(gpui::fill(
                            gpui::Bounds::new(
                                track.origin,
                                gpui::size(track.size.width * value, px(3.)),
                            ),
                            rgb(colors.accent),
                        ));
                        let ratio = move |point: gpui::Point<gpui::Pixels>| {
                            (f32::from(point.x - bounds.left())
                                / f32::from(bounds.size.width).max(1.))
                            .clamp(0., 1.)
                        };
                        let down = view.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                            if enabled
                                && phase == DispatchPhase::Bubble
                                && event.button == MouseButton::Left
                                && bounds.contains(&event.position)
                            {
                                let _ = down.update(cx, |slider, cx| {
                                    slider.begin(ratio(event.position), cx)
                                });
                                cx.stop_propagation();
                            }
                        });
                        let motion = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ = motion.update(cx, |slider, cx| {
                                    slider.motion(
                                        ratio(event.position),
                                        bounds.contains(&event.position),
                                        event.dragging(),
                                        cx,
                                    )
                                });
                            }
                        });
                        let up = view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                let _ = up.update(cx, |slider, cx| {
                                    slider.finish(ratio(event.position), cx)
                                });
                            }
                        });
                    },
                )
                .size_full(),
            )
            .when(thumb, |view| {
                view.child(
                    div()
                        .absolute()
                        .left(relative(value))
                        .ml(px(-4.))
                        .top(px(5.))
                        .size(px(8.))
                        .rounded_full()
                        .bg(rgb(colors.accent)),
                )
            })
            .when_some(
                duration.zip(hint).filter(|_| self.enabled),
                |view, (duration, ratio)| {
                    view.child(
                        div()
                            .absolute()
                            .left(relative(ratio.clamp(0.03, 0.97)))
                            .ml(px(-28.))
                            .bottom(px(22.))
                            .w(px(56.))
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors.tooltip_bg))
                            .text_color(rgb(colors.tooltip_text))
                            .text_xs()
                            .text_center()
                            .child(time_label((duration as f32 * ratio).round() as u64)),
                    )
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dragging_clamps_outside_bounds_and_commits_only_an_active_gesture() {
        let mut pointer = Interaction::default();
        assert_eq!(pointer.finish(0.8), None);
        pointer.begin(0.2);
        pointer.motion(1.4, true);
        assert_eq!(pointer.drag, Some(1.));
        assert_eq!(pointer.finish(-0.4), Some(0.));
        assert_eq!(pointer.finish(0.7), None);
    }
    #[test]
    fn lost_mouse_release_does_not_commit_a_stale_gesture() {
        let mut pointer = Interaction::default();
        pointer.begin(0.2);
        pointer.motion(0.9, false);
        assert_eq!(pointer.finish(0.9), None);
    }
}
