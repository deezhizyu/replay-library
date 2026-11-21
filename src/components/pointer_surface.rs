use std::rc::Rc;

use gpui::{
    App, Bounds, Div, ElementId, InteractiveElement as _, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, SharedString, Size, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    Hover,
    Leave,
    Down,
    Drag,
    Up,
}

#[derive(Clone, Copy)]
pub struct PointerEvent {
    pub phase: PointerPhase,
    pub position: Point<Pixels>,
    pub size: Size<Pixels>,
}

impl PointerEvent {
    pub fn fraction(&self) -> f32 {
        (self.position.x / self.size.width).clamp(0.0, 1.0)
    }
}

pub fn pointer_surface(
    id: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
    on_pointer: impl Fn(PointerEvent, &mut App) + 'static,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let tracked_bounds =
        window.use_keyed_state(ElementId::from((id.clone(), 0usize)), cx, |_, _| {
            Bounds::<Pixels>::default()
        });
    let dragging = window.use_keyed_state(ElementId::from((id.clone(), 1usize)), cx, |_, _| false);
    let report = Rc::new({
        let tracked_bounds = tracked_bounds.clone();
        move |phase: PointerPhase, position: Point<Pixels>, cx: &mut App| {
            let bounds = *tracked_bounds.read(cx);
            on_pointer(
                PointerEvent {
                    phase,
                    position: position - bounds.origin,
                    size: bounds.size,
                },
                cx,
            );
        }
    });
    let drag_listeners = canvas(|bounds, _, _| bounds, {
        let dragging = dragging.clone();
        let report = report.clone();
        move |_, bounds, window, cx| {
            tracked_bounds.update(cx, |stored, _| *stored = bounds);
            window.on_mouse_event({
                let dragging = dragging.clone();
                let report = report.clone();
                move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase.capture() || !*dragging.read(cx) {
                        return;
                    }
                    if event.dragging() {
                        report(PointerPhase::Drag, event.position, cx);
                    } else {
                        dragging.update(cx, |dragging, _| *dragging = false);
                        report(PointerPhase::Up, event.position, cx);
                    }
                }
            });
            window.on_mouse_event({
                let dragging = dragging.clone();
                let report = report.clone();
                move |event: &MouseUpEvent, phase, _, cx| {
                    if phase.bubble() && *dragging.read(cx) {
                        dragging.update(cx, |dragging, _| *dragging = false);
                        report(PointerPhase::Up, event.position, cx);
                    }
                }
            });
        }
    })
    .absolute()
    .size_full();

    div()
        .id(ElementId::from(id))
        .relative()
        .child(drag_listeners)
        .on_mouse_down(MouseButton::Left, {
            let dragging = dragging.clone();
            let report = report.clone();
            move |event, _, cx| {
                dragging.update(cx, |dragging, _| *dragging = true);
                report(PointerPhase::Down, event.position, cx);
            }
        })
        .on_mouse_move({
            let dragging = dragging.clone();
            let report = report.clone();
            move |event, _, cx| {
                if !*dragging.read(cx) {
                    report(PointerPhase::Hover, event.position, cx);
                }
            }
        })
        .on_hover(move |hovered, _, cx| {
            if !*hovered && !*dragging.read(cx) {
                report(PointerPhase::Leave, Point::default(), cx);
            }
        })
}
