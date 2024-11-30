//! Div is the central, reusable element that most GPUI trees will be built from.
//! It functions as a container for other elements, and provides a number of
//! useful features for laying out and styling its children as well as binding
//! mouse events and action handlers. It is meant to be similar to the HTML `<div>`
//! element, but for GPUI.
//!
//! # Build your own div
//!
//! GPUI does not directly provide APIs for stateful, multi step events like `click`
//! and `drag`. We want GPUI users to be able to build their own abstractions for
//! their own needs. However, as a UI framework, we're also obliged to provide some
//! building blocks to make the process of building your own elements easier.
//! For this we have the [`Interactivity`] and the [`StyleRefinement`] structs, as well
//! as several associated traits. Together, these provide the full suite of Dom-like events
//! and Tailwind-like styling that you can use to build your own custom elements. Div is
//! constructed by combining these two systems into an all-in-one element.

use crate::{
    point, px, size, Action, AnyDrag, AnyElement, AnyTooltip, AppContext, Bounds, ClickEvent,
    DispatchPhase, Element, ElementId, FocusHandle, Global, GlobalElementId, Hitbox, HitboxId,
    IntoElement, IsZero, KeyContext, KeyDownEvent, KeyUpEvent, LayoutId, ModifiersChangedEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    ScrollWheelEvent, SharedString, Size, Style, StyleRefinement, Styled, Task, TooltipId,
    Visibility, Window,
};
use collections::HashMap;
use refineable::Refineable;
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    cmp::Ordering,
    fmt::Debug,
    marker::PhantomData,
    mem,
    rc::Rc,
    time::Duration,
};
use taffy::style::Overflow;
use util::ResultExt;

const DRAG_THRESHOLD: f64 = 2.;
pub(crate) const TOOLTIP_DELAY: Duration = Duration::from_millis(500);

/// The styling information for a given group.
pub struct GroupStyle {
    /// The identifier for this group.
    pub group: SharedString,

    /// The specific style refinement that this group would apply
    /// to its children.
    pub style: Box<StyleRefinement>,
}

/// An event for when a drag is moving over this element, with the given state type.
pub struct DragMoveEvent<T> {
    /// The mouse move event that triggered this drag move event.
    pub event: MouseMoveEvent,

    /// The bounds of this element.
    pub bounds: Bounds<Pixels>,
    drag: PhantomData<T>,
}

impl<T: 'static> DragMoveEvent<T> {
    /// Returns the drag state for this event.
    pub fn drag<'b>(&self, cx: &'b AppContext) -> &'b T {
        cx.active_drag
            .as_ref()
            .and_then(|drag| drag.value.downcast_ref::<T>())
            .expect("DragMoveEvent is only valid when the stored active drag is of the same type.")
    }
}

impl Interactivity {
    /// Bind the given callback to the mouse down event for the given mouse button, during the bubble phase
    /// The imperative API equivalent of [`InteractiveElement::on_mouse_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to the view state from this callback.
    pub fn on_mouse_down(
        &mut self,
        button: MouseButton,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_down_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == button
                    && hitbox.is_hovered(window)
                {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse down event for any button, during the capture phase
    /// The imperative API equivalent of [`InteractiveElement::capture_any_mouse_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn capture_any_mouse_down(
        &mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_down_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Capture && hitbox.is_hovered(window) {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse down event for any button, during the bubble phase
    /// the imperative API equivalent to [`InteractiveElement::on_any_mouse_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_any_mouse_down(
        &mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_down_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse up event for the given button, during the bubble phase
    /// the imperative API equivalent to [`InteractiveElement::on_mouse_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_mouse_up(
        &mut self,
        button: MouseButton,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_up_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == button
                    && hitbox.is_hovered(window)
                {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse up event for any button, during the capture phase
    /// the imperative API equivalent to [`InteractiveElement::capture_any_mouse_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn capture_any_mouse_up(
        &mut self,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_up_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Capture && hitbox.is_hovered(window) {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse up event for any button, during the bubble phase
    /// the imperative API equivalent to [`Interactivity::on_any_mouse_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_any_mouse_up(
        &mut self,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_up_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse down event, on any button, during the capture phase,
    /// when the mouse is outside of the bounds of this element.
    /// The imperative API equivalent to [`InteractiveElement::on_mouse_down_out`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_mouse_down_out(
        &mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_down_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Capture && !hitbox.contains(&window.mouse_position()) {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to the mouse up event, for the given button, during the capture phase,
    /// when the mouse is outside of the bounds of this element.
    /// The imperative API equivalent to [`InteractiveElement::on_mouse_up_out`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_mouse_up_out(
        &mut self,
        button: MouseButton,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_up_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Capture
                    && event.button == button
                    && !hitbox.is_hovered(window)
                {
                    (listener)(event, window, cx);
                }
            }));
    }

    /// Bind the given callback to the mouse move event, during the bubble phase
    /// The imperative API equivalent to [`InteractiveElement::on_mouse_move`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_mouse_move(
        &mut self,
        listener: impl Fn(&MouseMoveEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.mouse_move_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    (listener)(event, window, cx);
                }
            }));
    }

    /// Bind the given callback to the mouse drag event of the given type. Note that this
    /// will be called for all move events, inside or outside of this element, as long as the
    /// drag was started with this element under the mouse. Useful for implementing draggable
    /// UIs that don't conform to a drag and drop style interaction, like resizing.
    /// The imperative API equivalent to [`InteractiveElement::on_drag_move`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_drag_move<T>(
        &mut self,
        listener: impl Fn(&DragMoveEvent<T>, &mut Window, &mut AppContext) + 'static,
    ) where
        T: 'static,
    {
        self.mouse_move_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Capture
                    && cx
                        .active_drag
                        .as_ref()
                        .is_some_and(|drag| drag.value.as_ref().type_id() == TypeId::of::<T>())
                {
                    (listener)(
                        &DragMoveEvent {
                            event: event.clone(),
                            bounds: hitbox.bounds,
                            drag: PhantomData,
                        },
                        window,
                        cx,
                    );
                }
            }));
    }

    /// Bind the given callback to scroll wheel events during the bubble phase
    /// The imperative API equivalent to [`InteractiveElement::on_scroll_wheel`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_scroll_wheel(
        &mut self,
        listener: impl Fn(&ScrollWheelEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.scroll_wheel_listeners
            .push(Box::new(move |event, phase, hitbox, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    (listener)(event, window, cx);
                }
            }));
    }

    /// Bind the given callback to an action dispatch during the capture phase
    /// The imperative API equivalent to [`InteractiveElement::capture_action`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn capture_action<A: Action>(
        &mut self,
        listener: impl Fn(&A, &mut Window, &mut AppContext) + 'static,
    ) {
        self.action_listeners.push((
            TypeId::of::<A>(),
            Box::new(move |action, phase, window, cx| {
                let action = action.downcast_ref().unwrap();
                if phase == DispatchPhase::Capture {
                    (listener)(action, window, cx)
                } else {
                    cx.propagate();
                }
            }),
        ));
    }

    /// Bind the given callback to an action dispatch during the bubble phase
    /// The imperative API equivalent to [`InteractiveElement::on_action`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_action<A: Action>(
        &mut self,
        listener: impl Fn(&A, &mut Window, &mut AppContext) + 'static,
    ) {
        self.action_listeners.push((
            TypeId::of::<A>(),
            Box::new(move |action, phase, window, cx| {
                let action = action.downcast_ref().unwrap();
                if phase == DispatchPhase::Bubble {
                    (listener)(action, window, cx)
                }
            }),
        ));
    }

    /// Bind the given callback to an action dispatch, based on a dynamic action parameter
    /// instead of a type parameter. Useful for component libraries that want to expose
    /// action bindings to their users.
    /// The imperative API equivalent to [`InteractiveElement::on_boxed_action`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_boxed_action(
        &mut self,
        action: &dyn Action,
        listener: impl Fn(&dyn Action, &mut Window, &mut AppContext) + 'static,
    ) {
        let action = action.boxed_clone();
        self.action_listeners.push((
            (*action).type_id(),
            Box::new(move |_, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    (listener)(&*action, window, cx)
                }
            }),
        ));
    }

    /// Bind the given callback to key down events during the bubble phase
    /// The imperative API equivalent to [`InteractiveElement::on_key_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_key_down(
        &mut self,
        listener: impl Fn(&KeyDownEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.key_down_listeners
            .push(Box::new(move |event, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    (listener)(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to key down events during the capture phase
    /// The imperative API equivalent to [`InteractiveElement::capture_key_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn capture_key_down(
        &mut self,
        listener: impl Fn(&KeyDownEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.key_down_listeners
            .push(Box::new(move |event, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    listener(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to key up events during the bubble phase
    /// The imperative API equivalent to [`InteractiveElement::on_key_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_key_up(
        &mut self,
        listener: impl Fn(&KeyUpEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.key_up_listeners
            .push(Box::new(move |event, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    listener(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to key up events during the capture phase
    /// The imperative API equivalent to [`InteractiveElement::on_key_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn capture_key_up(
        &mut self,
        listener: impl Fn(&KeyUpEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.key_up_listeners
            .push(Box::new(move |event, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    listener(event, window, cx)
                }
            }));
    }

    /// Bind the given callback to modifiers changing events.
    /// The imperative API equivalent to [`InteractiveElement::on_modifiers_changed`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_modifiers_changed(
        &mut self,
        listener: impl Fn(&ModifiersChangedEvent, &mut Window, &mut AppContext) + 'static,
    ) {
        self.modifiers_changed_listeners
            .push(Box::new(move |event, window, cx| {
                listener(event, window, cx)
            }));
    }

    /// Bind the given callback to drop events of the given type, whether or not the drag started on this element
    /// The imperative API equivalent to [`InteractiveElement::on_drop`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_drop<T: 'static>(
        &mut self,
        listener: impl Fn(&T, &mut Window, &mut AppContext) + 'static,
    ) {
        self.drop_listeners.push((
            TypeId::of::<T>(),
            Box::new(move |dragged_value, window, cx| {
                listener(dragged_value.downcast_ref().unwrap(), window, cx);
            }),
        ));
    }

    /// Use the given predicate to determine whether or not a drop event should be dispatched to this element
    /// The imperative API equivalent to [`InteractiveElement::can_drop`]
    pub fn can_drop(
        &mut self,
        predicate: impl Fn(&dyn Any, &mut Window, &mut AppContext) -> bool + 'static,
    ) {
        self.can_drop_predicate = Some(Box::new(move |value, window, cx| {
            predicate(value, window, cx)
        }));
    }

    /// Bind the given callback to click events of this element
    /// The imperative API equivalent to [`StatefulInteractiveElement::on_click`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_click(
        &mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut AppContext) + 'static,
    ) where
        Self: Sized,
    {
        self.click_listeners
            .push(Box::new(move |event, window, cx| {
                listener(event, window, cx)
            }));
    }

    /// On drag initiation, this callback will be used to create a new view to render the dragged value for a
    /// drag and drop operation. This API should also be used as the equivalent of 'on drag start' with
    /// the [`Self::on_drag_move`] API
    /// The imperative API equivalent to [`StatefulInteractiveElement::on_drag`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_drag<T, F, E>(
        &mut self,
        value: T,
        listener: impl 'static + Fn(&T, Point<Pixels>, &mut Window, &mut AppContext) -> F,
    ) where
        Self: Sized,
        T: 'static,
        F: 'static + Fn(&T, Point<Pixels>, &mut Window, &mut AppContext) -> E,
        E: IntoElement,
    {
        debug_assert!(
            self.drag_listener.is_none(),
            "calling on_drag more than once on the same element is not supported"
        );
        self.drag_listener = Some((
            Box::new(value),
            Box::new(move |value, offset, window, cx| {
                let renderer = listener(value.downcast_ref().unwrap(), offset, window, cx);
                Box::new(move |value, offset, window, cx| {
                    renderer(value.downcast_ref().unwrap(), offset, window, cx).into_any_element()
                })
            }),
        ));
    }

    /// Bind the given callback on the hover start and end events of this element. Note that the boolean
    /// passed to the callback is true when the hover starts and false when it ends.
    /// The imperative API equivalent to [`StatefulInteractiveElement::on_drag`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    pub fn on_hover(&mut self, listener: impl Fn(&bool, &mut Window, &mut AppContext) + 'static)
    where
        Self: Sized,
    {
        debug_assert!(
            self.hover_listener.is_none(),
            "calling on_hover more than once on the same element is not supported"
        );
        self.hover_listener = Some(Box::new(listener));
    }

    /// Use the given callback to construct a new tooltip view when the mouse hovers over this element.
    /// The imperative API equivalent to [`InteractiveElement::tooltip`]
    pub fn tooltip<F, E>(
        &mut self,
        build_tooltip: impl 'static + Fn(&mut Window, &mut AppContext) -> F,
    ) where
        Self: Sized,
        F: 'static + Fn(&mut Window, &mut AppContext) -> E,
        E: IntoElement,
    {
        debug_assert!(
            self.tooltip_builder.is_none(),
            "calling tooltip more than once on the same element is not supported"
        );
        self.tooltip_builder = Some(TooltipBuilder {
            build: Rc::new(move |window, cx| {
                let render_tooltip = build_tooltip(window, cx);
                Rc::new(move |window, cx| render_tooltip(window, cx).into_any_element())
            }),
            hoverable: false,
        });
    }

    /// Use the given callback to construct a new tooltip view when the mouse hovers over this element.
    /// The tooltip itself is also hoverable and won't disappear when the user moves the mouse into
    /// the tooltip. The imperative API equivalent to [`InteractiveElement::hoverable_tooltip`]
    pub fn hoverable_tooltip<F, E>(
        &mut self,
        build_tooltip: impl 'static + Fn(&mut Window, &mut AppContext) -> F,
    ) where
        Self: Sized,
        F: 'static + Fn(&mut Window, &mut AppContext) -> E,
        E: IntoElement,
    {
        debug_assert!(
            self.tooltip_builder.is_none(),
            "calling tooltip more than once on the same element is not supported"
        );
        self.tooltip_builder = Some(TooltipBuilder {
            build: Rc::new(move |window, cx| {
                let render_tooltip = build_tooltip(window, cx);
                Rc::new(move |window, cx| render_tooltip(window, cx).into_any_element())
            }),
            hoverable: true,
        });
    }

    /// Block the mouse from interacting with this element or any of its children
    /// The imperative API equivalent to [`InteractiveElement::occlude`]
    pub fn occlude_mouse(&mut self) {
        self.occlude_mouse = true;
    }
}

/// A trait for elements that want to use the standard GPUI event handlers that don't
/// require any state.
pub trait InteractiveElement: Sized {
    /// Retrieve the interactivity state associated with this element
    fn interactivity(&mut self) -> &mut Interactivity;

    /// Assign this element to a group of elements that can be styled together
    fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.interactivity().group = Some(group.into());
        self
    }

    /// Assign this element an ID, so that it can be used with interactivity
    fn id(mut self, id: impl Into<ElementId>) -> Stateful<Self> {
        self.interactivity().element_id = Some(id.into());

        Stateful { element: self }
    }

    /// Track the focus state of the given focus handle on this element.
    /// If the focus handle is focused by the application, this element will
    /// apply its focused styles.
    fn track_focus(mut self, focus_handle: &FocusHandle) -> Focusable<Self> {
        self.interactivity().focusable = true;
        self.interactivity().tracked_focus_handle = Some(focus_handle.clone());
        Focusable { element: self }
    }

    /// Set the keymap context for this element. This will be used to determine
    /// which action to dispatch from the keymap.
    fn key_context<C, E>(mut self, key_context: C) -> Self
    where
        C: TryInto<KeyContext, Error = E>,
        E: Debug,
    {
        if let Some(key_context) = key_context.try_into().log_err() {
            self.interactivity().key_context = Some(key_context);
        }
        self
    }

    /// Apply the given style to this element when the mouse hovers over it
    fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        debug_assert!(
            self.interactivity().hover_style.is_none(),
            "hover style already set"
        );
        self.interactivity().hover_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Apply the given style to this element when the mouse hovers over a group member
    fn group_hover(
        mut self,
        group_name: impl Into<SharedString>,
        f: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self {
        self.interactivity().group_hover_style = Some(GroupStyle {
            group: group_name.into(),
            style: Box::new(f(StyleRefinement::default())),
        });
        self
    }

    /// Bind the given callback to the mouse down event for the given mouse button,
    /// the fluent API equivalent to [`Interactivity::on_mouse_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to the view state from this callback.
    fn on_mouse_down(
        mut self,
        button: MouseButton,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_mouse_down(button, listener);
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    /// Set a key that can be used to look up this element's bounds
    /// in the [`VisualTestContext::debug_bounds`] map
    /// This is a noop in release builds
    fn debug_selector(mut self, f: impl FnOnce() -> String) -> Self {
        self.interactivity().debug_selector = Some(f());
        self
    }

    #[cfg(not(any(test, feature = "test-support")))]
    /// Set a key that can be used to look up this element's bounds
    /// in the [`VisualTestContext::debug_bounds`] map
    /// This is a noop in release builds
    #[inline]
    fn debug_selector(self, _: impl FnOnce() -> String) -> Self {
        self
    }

    /// Bind the given callback to the mouse down event for any button, during the capture phase
    /// the fluent API equivalent to [`Interactivity::capture_any_mouse_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn capture_any_mouse_down(
        mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().capture_any_mouse_down(listener);
        self
    }

    /// Bind the given callback to the mouse down event for any button, during the capture phase
    /// the fluent API equivalent to [`Interactivity::on_any_mouse_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_any_mouse_down(
        mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_any_mouse_down(listener);
        self
    }

    /// Bind the given callback to the mouse up event for the given button, during the bubble phase
    /// the fluent API equivalent to [`Interactivity::on_mouse_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_mouse_up(
        mut self,
        button: MouseButton,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_mouse_up(button, listener);
        self
    }

    /// Bind the given callback to the mouse up event for any button, during the capture phase
    /// the fluent API equivalent to [`Interactivity::capture_any_mouse_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn capture_any_mouse_up(
        mut self,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().capture_any_mouse_up(listener);
        self
    }

    /// Bind the given callback to the mouse down event, on any button, during the capture phase,
    /// when the mouse is outside of the bounds of this element.
    /// The fluent API equivalent to [`Interactivity::on_mouse_down_out`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_mouse_down_out(
        mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_mouse_down_out(listener);
        self
    }

    /// Bind the given callback to the mouse up event, for the given button, during the capture phase,
    /// when the mouse is outside of the bounds of this element.
    /// The fluent API equivalent to [`Interactivity::on_mouse_up_out`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_mouse_up_out(
        mut self,
        button: MouseButton,
        listener: impl Fn(&MouseUpEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_mouse_up_out(button, listener);
        self
    }

    /// Bind the given callback to the mouse move event, during the bubble phase
    /// The fluent API equivalent to [`Interactivity::on_mouse_move`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_mouse_move(
        mut self,
        listener: impl Fn(&MouseMoveEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_mouse_move(listener);
        self
    }

    /// Bind the given callback to the mouse drag event of the given type. Note that this
    /// will be called for all move events, inside or outside of this element, as long as the
    /// drag was started with this element under the mouse. Useful for implementing draggable
    /// UIs that don't conform to a drag and drop style interaction, like resizing.
    /// The fluent API equivalent to [`Interactivity::on_drag_move`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_drag_move<T: 'static>(
        mut self,
        listener: impl Fn(&DragMoveEvent<T>, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_drag_move(listener);
        self
    }

    /// Bind the given callback to scroll wheel events during the bubble phase
    /// The fluent API equivalent to [`Interactivity::on_scroll_wheel`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_scroll_wheel(
        mut self,
        listener: impl Fn(&ScrollWheelEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_scroll_wheel(listener);
        self
    }

    /// Capture the given action, before normal action dispatch can fire
    /// The fluent API equivalent to [`Interactivity::on_scroll_wheel`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn capture_action<A: Action>(
        mut self,
        listener: impl Fn(&A, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().capture_action(listener);
        self
    }

    /// Bind the given callback to an action dispatch during the bubble phase
    /// The fluent API equivalent to [`Interactivity::on_action`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_action<A: Action>(
        mut self,
        listener: impl Fn(&A, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_action(listener);
        self
    }

    /// Bind the given callback to an action dispatch, based on a dynamic action parameter
    /// instead of a type parameter. Useful for component libraries that want to expose
    /// action bindings to their users.
    /// The fluent API equivalent to [`Interactivity::on_boxed_action`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_boxed_action(
        mut self,
        action: &dyn Action,
        listener: impl Fn(&dyn Action, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_boxed_action(action, listener);
        self
    }

    /// Bind the given callback to key down events during the bubble phase
    /// The fluent API equivalent to [`Interactivity::on_key_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_key_down(
        mut self,
        listener: impl Fn(&KeyDownEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_key_down(listener);
        self
    }

    /// Bind the given callback to key down events during the capture phase
    /// The fluent API equivalent to [`Interactivity::capture_key_down`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn capture_key_down(
        mut self,
        listener: impl Fn(&KeyDownEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().capture_key_down(listener);
        self
    }

    /// Bind the given callback to key up events during the bubble phase
    /// The fluent API equivalent to [`Interactivity::on_key_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_key_up(
        mut self,
        listener: impl Fn(&KeyUpEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_key_up(listener);
        self
    }

    /// Bind the given callback to key up events during the capture phase
    /// The fluent API equivalent to [`Interactivity::capture_key_up`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn capture_key_up(
        mut self,
        listener: impl Fn(&KeyUpEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().capture_key_up(listener);
        self
    }

    /// Bind the given callback to modifiers changing events.
    /// The fluent API equivalent to [`Interactivity::on_modifiers_changed`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_modifiers_changed(
        mut self,
        listener: impl Fn(&ModifiersChangedEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_modifiers_changed(listener);
        self
    }

    /// Apply the given style when the given data type is dragged over this element
    fn drag_over<S: 'static>(
        mut self,
        f: impl 'static + Fn(StyleRefinement, &S, &Window, &AppContext) -> StyleRefinement,
    ) -> Self {
        self.interactivity().drag_over_styles.push((
            TypeId::of::<S>(),
            Box::new(move |currently_dragged: &dyn Any, window, cx| {
                f(
                    StyleRefinement::default(),
                    currently_dragged.downcast_ref::<S>().unwrap(),
                    window,
                    cx,
                )
            }),
        ));
        self
    }

    /// Apply the given style when the given data type is dragged over this element's group
    fn group_drag_over<S: 'static>(
        mut self,
        group_name: impl Into<SharedString>,
        f: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self {
        self.interactivity().group_drag_over_styles.push((
            TypeId::of::<S>(),
            GroupStyle {
                group: group_name.into(),
                style: Box::new(f(StyleRefinement::default())),
            },
        ));
        self
    }

    /// Bind the given callback to drop events of the given type, whether or not the drag started on this element
    /// The fluent API equivalent to [`Interactivity::on_drop`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_drop<T: 'static>(
        mut self,
        listener: impl Fn(&T, &mut Window, &mut AppContext) + 'static,
    ) -> Self {
        self.interactivity().on_drop(listener);
        self
    }

    /// Use the given predicate to determine whether or not a drop event should be dispatched to this element
    /// The fluent API equivalent to [`Interactivity::can_drop`]
    fn can_drop(
        mut self,
        predicate: impl Fn(&dyn Any, &mut Window, &mut AppContext) -> bool + 'static,
    ) -> Self {
        self.interactivity().can_drop(predicate);
        self
    }

    /// Block the mouse from interacting with this element or any of its children
    /// The fluent API equivalent to [`Interactivity::occlude_mouse`]
    fn occlude(mut self) -> Self {
        self.interactivity().occlude_mouse();
        self
    }

    /// Block the mouse from interacting with this element or any of its children
    /// The fluent API equivalent to [`Interactivity::occlude_mouse`]
    fn block_mouse_down(mut self) -> Self {
        self.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }
}

/// A trait for elements that want to use the standard GPUI interactivity features
/// that require state.
pub trait StatefulInteractiveElement: InteractiveElement {
    /// Set this element to focusable.
    fn focusable(mut self) -> Focusable<Self> {
        self.interactivity().focusable = true;
        Focusable { element: self }
    }

    /// Set the overflow x and y to scroll.
    fn overflow_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.x = Some(Overflow::Scroll);
        self.interactivity().base_style.overflow.y = Some(Overflow::Scroll);
        self
    }

    /// Set the overflow x to scroll.
    fn overflow_x_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.x = Some(Overflow::Scroll);
        self
    }

    /// Set the overflow y to scroll.
    fn overflow_y_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.y = Some(Overflow::Scroll);
        self
    }

    /// Track the scroll state of this element with the given handle.
    fn track_scroll(mut self, scroll_handle: &ScrollHandle) -> Self {
        self.interactivity().tracked_scroll_handle = Some(scroll_handle.clone());
        self
    }

    /// Track the scroll state of this element with the given handle.
    fn anchor_scroll(mut self, scroll_anchor: Option<ScrollAnchor>) -> Self {
        self.interactivity().scroll_anchor = scroll_anchor;
        self
    }

    /// Set the given styles to be applied when this element is active.
    fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self
    where
        Self: Sized,
    {
        self.interactivity().active_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Set the given styles to be applied when this element's group is active.
    fn group_active(
        mut self,
        group_name: impl Into<SharedString>,
        f: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self
    where
        Self: Sized,
    {
        self.interactivity().group_active_style = Some(GroupStyle {
            group: group_name.into(),
            style: Box::new(f(StyleRefinement::default())),
        });
        self
    }

    /// Bind the given callback to click events of this element
    /// The fluent API equivalent to [`Interactivity::on_click`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut AppContext) + 'static,
    ) -> Self
    where
        Self: Sized,
    {
        self.interactivity().on_click(listener);
        self
    }

    /// On drag initiation, this callback will be used to create a new view to render the dragged value for a
    /// drag and drop operation. This API should also be used as the equivalent of 'on drag start' with
    /// the [`Self::on_drag_move`] API.
    /// The callback also has access to the offset of triggering click from the origin of parent element.
    /// The fluent API equivalent to [`Interactivity::on_drag`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_drag<T, F, E>(
        mut self,
        value: T,
        listener: impl Fn(&T, Point<Pixels>, &mut Window, &mut AppContext) -> F + 'static,
    ) -> Self
    where
        Self: Sized,
        T: 'static,
        F: Fn(&T, Point<Pixels>, &mut Window, &mut AppContext) -> E + 'static,
        E: IntoElement,
    {
        self.interactivity().on_drag(value, listener);
        self
    }

    /// Bind the given callback on the hover start and end events of this element. Note that the boolean
    /// passed to the callback is true when the hover starts and false when it ends.
    /// The fluent API equivalent to [`Interactivity::on_hover`]
    ///
    /// See [`ViewContext::listener`](crate::ViewContext::listener) to get access to a view's state from this callback.
    fn on_hover(mut self, listener: impl 'static + Fn(&bool, &mut Window, &mut AppContext)) -> Self
    where
        Self: Sized,
    {
        self.interactivity().on_hover(listener);
        self
    }

    /// Use the given callback to construct a new tooltip view when the mouse hovers over this element.
    /// The fluent API equivalent to [`Interactivity::tooltip`]
    fn tooltip<F, E>(
        mut self,
        build_tooltip: impl 'static + Fn(&mut Window, &mut AppContext) -> F,
    ) -> Self
    where
        Self: Sized,
        F: 'static + Fn(&mut Window, &mut AppContext) -> E,
        E: IntoElement,
    {
        self.interactivity().tooltip(build_tooltip);
        self
    }

    /// Use the given callback to construct a new tooltip view when the mouse hovers over this element.
    /// The tooltip itself is also hoverable and won't disappear when the user moves the mouse into
    /// the tooltip. The fluent API equivalent to [`Interactivity::hoverable_tooltip`]
    fn hoverable_tooltip<F, E>(
        mut self,
        build_tooltip: impl 'static + Fn(&mut Window, &mut AppContext) -> F,
    ) -> Self
    where
        Self: Sized,
        F: 'static + Fn(&mut Window, &mut AppContext) -> E,
        E: IntoElement,
    {
        self.interactivity().hoverable_tooltip(build_tooltip);
        self
    }
}

/// A trait for providing focus related APIs to interactive elements
pub trait FocusableElement: InteractiveElement {
    /// Set the given styles to be applied when this element, specifically, is focused.
    fn focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self
    where
        Self: Sized,
    {
        self.interactivity().focus_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Set the given styles to be applied when this element is inside another element that is focused.
    fn in_focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self
    where
        Self: Sized,
    {
        self.interactivity().in_focus_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }
}

pub(crate) type MouseDownListener =
    Box<dyn Fn(&MouseDownEvent, DispatchPhase, &Hitbox, &mut Window, &mut AppContext) + 'static>;
pub(crate) type MouseUpListener =
    Box<dyn Fn(&MouseUpEvent, DispatchPhase, &Hitbox, &mut Window, &mut AppContext) + 'static>;

pub(crate) type MouseMoveListener =
    Box<dyn Fn(&MouseMoveEvent, DispatchPhase, &Hitbox, &mut Window, &mut AppContext) + 'static>;

pub(crate) type ScrollWheelListener =
    Box<dyn Fn(&ScrollWheelEvent, DispatchPhase, &Hitbox, &mut Window, &mut AppContext) + 'static>;

pub(crate) type ClickListener = Box<dyn Fn(&ClickEvent, &mut Window, &mut AppContext) + 'static>;

pub(crate) type DragRenderer =
    Box<dyn 'static + Fn(&dyn Any, Point<Pixels>, &mut Window, &mut AppContext) -> AnyElement>;

pub(crate) type DragListener =
    Box<dyn Fn(&dyn Any, Point<Pixels>, &mut Window, &mut AppContext) -> DragRenderer>;

type DropListener = Box<dyn Fn(&dyn Any, &mut Window, &mut AppContext) + 'static>;

type CanDropPredicate = Box<dyn Fn(&dyn Any, &mut Window, &mut AppContext) -> bool + 'static>;

pub(crate) struct TooltipBuilder {
    build: Rc<
        dyn Fn(
            &mut Window,
            &mut AppContext,
        ) -> Rc<dyn Fn(&mut Window, &mut AppContext) -> AnyElement>,
    >,
    hoverable: bool,
}

pub(crate) type KeyDownListener =
    Box<dyn Fn(&KeyDownEvent, DispatchPhase, &mut Window, &mut AppContext) + 'static>;

pub(crate) type KeyUpListener =
    Box<dyn Fn(&KeyUpEvent, DispatchPhase, &mut Window, &mut AppContext) + 'static>;

pub(crate) type ModifiersChangedListener =
    Box<dyn Fn(&ModifiersChangedEvent, &mut Window, &mut AppContext) + 'static>;

pub(crate) type ActionListener =
    Box<dyn Fn(&dyn Any, DispatchPhase, &mut Window, &mut AppContext) + 'static>;

/// Construct a new [`Div`] element
#[track_caller]
pub fn div() -> Div {
    #[cfg(debug_assertions)]
    let interactivity = Interactivity {
        location: Some(*core::panic::Location::caller()),
        ..Default::default()
    };

    #[cfg(not(debug_assertions))]
    let interactivity = Interactivity::default();

    Div {
        interactivity,
        children: SmallVec::default(),
    }
}

/// A [`Div`] element, the all-in-one element for building complex UIs in GPUI
pub struct Div {
    interactivity: Interactivity,
    children: SmallVec<[AnyElement; 2]>,
}

/// A frame state for a `Div` element, which contains layout IDs for its children.
///
/// This struct is used internally by the `Div` element to manage the layout state of its children
/// during the UI update cycle. It holds a small vector of `LayoutId` values, each corresponding to
/// a child element of the `Div`. These IDs are used to query the layout engine for the computed
/// bounds of the children after the layout phase is complete.
pub struct DivFrameState {
    child_layout_ids: SmallVec<[LayoutId; 2]>,
}

impl Styled for Div {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl InteractiveElement for Div {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

impl ParentElement for Div {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Element for Div {
    type RequestLayoutState = DivFrameState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        self.interactivity.element_id.clone()
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child_layout_ids = SmallVec::new();
        let layout_id =
            self.interactivity
                .request_layout(global_id, window, cx, |style, window, cx| {
                    window.with_text_style(style.text_style().cloned(), |window| {
                        child_layout_ids = self
                            .children
                            .iter_mut()
                            .map(|child| child.request_layout(window, cx))
                            .collect::<SmallVec<_>>();
                        window.request_layout(style, child_layout_ids.iter().copied(), cx)
                    })
                });
        (layout_id, DivFrameState { child_layout_ids })
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<Hitbox> {
        let mut child_min = point(Pixels::MAX, Pixels::MAX);
        let mut child_max = Point::default();
        if let Some(handle) = self.interactivity.scroll_anchor.as_ref() {
            *handle.last_origin.borrow_mut() = bounds.origin - window.element_offset();
        }
        let content_size = if request_layout.child_layout_ids.is_empty() {
            bounds.size
        } else if let Some(scroll_handle) = self.interactivity.tracked_scroll_handle.as_ref() {
            let mut state = scroll_handle.0.borrow_mut();
            state.child_bounds = Vec::with_capacity(request_layout.child_layout_ids.len());
            state.bounds = bounds;
            let requested = state.requested_scroll_top.take();

            for (ix, child_layout_id) in request_layout.child_layout_ids.iter().enumerate() {
                let child_bounds = window.layout_bounds(*child_layout_id);
                child_min = child_min.min(&child_bounds.origin);
                child_max = child_max.max(&child_bounds.lower_right());
                state.child_bounds.push(child_bounds);

                if let Some(requested) = requested.as_ref() {
                    if requested.0 == ix {
                        *state.offset.borrow_mut() =
                            bounds.origin - (child_bounds.origin - point(px(0.), requested.1));
                    }
                }
            }
            (child_max - child_min).into()
        } else {
            for child_layout_id in &request_layout.child_layout_ids {
                let child_bounds = window.layout_bounds(*child_layout_id);
                child_min = child_min.min(&child_bounds.origin);
                child_max = child_max.max(&child_bounds.lower_right());
            }
            (child_max - child_min).into()
        };

        self.interactivity.prepaint(
            global_id,
            bounds,
            content_size,
            window,
            cx,
            |_style, scroll_offset, hitbox, window, cx| {
                window.with_element_offset(scroll_offset, |window| {
                    for child in &mut self.children {
                        child.prepaint(window, cx);
                    }
                });
                hitbox
            },
        )
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.interactivity.paint(
            global_id,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |_style, window, cx| {
                for child in &mut self.children {
                    child.paint(window, cx);
                }
            },
        );
    }
}

impl IntoElement for Div {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The interactivity struct. Powers all of the general-purpose
/// interactivity in the `Div` element.
#[derive(Default)]
pub struct Interactivity {
    /// The element ID of the element. In id is required to support a stateful subset of the interactivity such as on_click.
    pub element_id: Option<ElementId>,
    /// Whether the element was clicked. This will only be present after layout.
    pub active: Option<bool>,
    /// Whether the element was hovered. This will only be present after paint if an hitbox
    /// was created for the interactive element.
    pub hovered: Option<bool>,
    pub(crate) tooltip_id: Option<TooltipId>,
    pub(crate) content_size: Size<Pixels>,
    pub(crate) key_context: Option<KeyContext>,
    pub(crate) focusable: bool,
    pub(crate) tracked_focus_handle: Option<FocusHandle>,
    pub(crate) tracked_scroll_handle: Option<ScrollHandle>,
    pub(crate) scroll_anchor: Option<ScrollAnchor>,
    pub(crate) scroll_offset: Option<Rc<RefCell<Point<Pixels>>>>,
    pub(crate) group: Option<SharedString>,
    /// The base style of the element, before any modifications are applied
    /// by focus, active, etc.
    pub base_style: Box<StyleRefinement>,
    pub(crate) focus_style: Option<Box<StyleRefinement>>,
    pub(crate) in_focus_style: Option<Box<StyleRefinement>>,
    pub(crate) hover_style: Option<Box<StyleRefinement>>,
    pub(crate) group_hover_style: Option<GroupStyle>,
    pub(crate) active_style: Option<Box<StyleRefinement>>,
    pub(crate) group_active_style: Option<GroupStyle>,
    pub(crate) drag_over_styles: Vec<(
        TypeId,
        Box<dyn Fn(&dyn Any, &mut Window, &mut AppContext) -> StyleRefinement>,
    )>,
    pub(crate) group_drag_over_styles: Vec<(TypeId, GroupStyle)>,
    pub(crate) mouse_down_listeners: Vec<MouseDownListener>,
    pub(crate) mouse_up_listeners: Vec<MouseUpListener>,
    pub(crate) mouse_move_listeners: Vec<MouseMoveListener>,
    pub(crate) scroll_wheel_listeners: Vec<ScrollWheelListener>,
    pub(crate) key_down_listeners: Vec<KeyDownListener>,
    pub(crate) key_up_listeners: Vec<KeyUpListener>,
    pub(crate) modifiers_changed_listeners: Vec<ModifiersChangedListener>,
    pub(crate) action_listeners: Vec<(TypeId, ActionListener)>,
    pub(crate) drop_listeners: Vec<(TypeId, DropListener)>,
    pub(crate) can_drop_predicate: Option<CanDropPredicate>,
    pub(crate) click_listeners: Vec<ClickListener>,
    pub(crate) drag_listener: Option<(Box<dyn Any>, DragListener)>,
    pub(crate) hover_listener: Option<Box<dyn Fn(&bool, &mut Window, &mut AppContext)>>,
    pub(crate) tooltip_builder: Option<TooltipBuilder>,
    pub(crate) occlude_mouse: bool,

    #[cfg(debug_assertions)]
    pub(crate) location: Option<core::panic::Location<'static>>,

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) debug_selector: Option<String>,
}

impl Interactivity {
    /// Layout this element according to this interactivity state's configured styles
    pub fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        window: &mut Window,
        cx: &mut AppContext,
        f: impl FnOnce(Style, &mut Window, &mut AppContext) -> LayoutId,
    ) -> LayoutId {
        window.with_optional_element_state::<InteractiveElementState, _>(
            global_id,
            |element_state, window| {
                let mut element_state =
                    element_state.map(|element_state| element_state.unwrap_or_default());

                if let Some(element_state) = element_state.as_ref() {
                    if cx.has_active_drag() {
                        if let Some(pending_mouse_down) = element_state.pending_mouse_down.as_ref()
                        {
                            *pending_mouse_down.borrow_mut() = None;
                        }
                        if let Some(clicked_state) = element_state.clicked_state.as_ref() {
                            *clicked_state.borrow_mut() = ElementClickedState::default();
                        }
                    }
                }

                // Ensure we store a focus handle in our element state if we're focusable.
                // If there's an explicit focus handle we're tracking, use that. Otherwise
                // create a new handle and store it in the element state, which lives for as
                // as frames contain an element with this id.
                if self.focusable && self.tracked_focus_handle.is_none() {
                    if let Some(element_state) = element_state.as_mut() {
                        self.tracked_focus_handle = Some(
                            element_state
                                .focus_handle
                                .get_or_insert_with(|| window.focus_handle())
                                .clone(),
                        );
                    }
                }

                if let Some(scroll_handle) = self.tracked_scroll_handle.as_ref() {
                    self.scroll_offset = Some(scroll_handle.0.borrow().offset.clone());
                } else if self.base_style.overflow.x == Some(Overflow::Scroll)
                    || self.base_style.overflow.y == Some(Overflow::Scroll)
                {
                    if let Some(element_state) = element_state.as_mut() {
                        self.scroll_offset = Some(
                            element_state
                                .scroll_offset
                                .get_or_insert_with(Rc::default)
                                .clone(),
                        );
                    }
                }

                let style = self.compute_style_internal(None, element_state.as_mut(), window, cx);
                let layout_id = f(style, window, cx);
                (layout_id, element_state)
            },
        )
    }

    /// Commit the bounds of this element according to this interactivity state's configured styles.
    pub fn prepaint<R>(
        &mut self,
        global_id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        content_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut AppContext,
        f: impl FnOnce(&Style, Point<Pixels>, Option<Hitbox>, &mut Window, &mut AppContext) -> R,
    ) -> R {
        self.content_size = content_size;
        if let Some(focus_handle) = self.tracked_focus_handle.as_ref() {
            window.set_focus_handle(focus_handle);
        }
        window.with_optional_element_state::<InteractiveElementState, _>(
            global_id,
            |element_state, window| {
                let mut element_state =
                    element_state.map(|element_state| element_state.unwrap_or_default());
                let style = self.compute_style_internal(None, element_state.as_mut(), window, cx);

                if let Some(element_state) = element_state.as_ref() {
                    if let Some(clicked_state) = element_state.clicked_state.as_ref() {
                        let clicked_state = clicked_state.borrow();
                        self.active = Some(clicked_state.element);
                    }

                    if let Some(active_tooltip) = element_state.active_tooltip.as_ref() {
                        if let Some(active_tooltip) = active_tooltip.borrow().as_ref() {
                            if let Some(tooltip) = active_tooltip.tooltip.clone() {
                                self.tooltip_id = Some(window.set_tooltip(tooltip));
                            }
                        }
                    }
                }

                window.with_text_style(style.text_style().cloned(), |window| {
                    window.with_content_mask(
                        style.overflow_mask(bounds, window.rem_size()),
                        |window| {
                            let hitbox = if self.should_insert_hitbox(&style) {
                                Some(window.insert_hitbox(bounds, self.occlude_mouse))
                            } else {
                                None
                            };

                            let scroll_offset = self.clamp_scroll_position(bounds, &style, window);
                            let result = f(&style, scroll_offset, hitbox, window, cx);
                            (result, element_state)
                        },
                    )
                })
            },
        )
    }

    fn should_insert_hitbox(&self, style: &Style) -> bool {
        self.occlude_mouse
            || style.mouse_cursor.is_some()
            || self.group.is_some()
            || self.scroll_offset.is_some()
            || self.tracked_focus_handle.is_some()
            || self.hover_style.is_some()
            || self.group_hover_style.is_some()
            || !self.mouse_up_listeners.is_empty()
            || !self.mouse_down_listeners.is_empty()
            || !self.mouse_move_listeners.is_empty()
            || !self.click_listeners.is_empty()
            || !self.scroll_wheel_listeners.is_empty()
            || self.drag_listener.is_some()
            || !self.drop_listeners.is_empty()
            || self.tooltip_builder.is_some()
    }

    fn clamp_scroll_position(
        &self,
        bounds: Bounds<Pixels>,
        style: &Style,
        window: &mut Window,
    ) -> Point<Pixels> {
        if let Some(scroll_offset) = self.scroll_offset.as_ref() {
            if let Some(scroll_handle) = &self.tracked_scroll_handle {
                scroll_handle.0.borrow_mut().overflow = style.overflow;
            }

            let rem_size = window.rem_size();
            let padding_size = size(
                style
                    .padding
                    .left
                    .to_pixels(bounds.size.width.into(), rem_size)
                    + style
                        .padding
                        .right
                        .to_pixels(bounds.size.width.into(), rem_size),
                style
                    .padding
                    .top
                    .to_pixels(bounds.size.height.into(), rem_size)
                    + style
                        .padding
                        .bottom
                        .to_pixels(bounds.size.height.into(), rem_size),
            );
            let scroll_max = (self.content_size + padding_size - bounds.size).max(&Size::default());
            // Clamp scroll offset in case scroll max is smaller now (e.g., if children
            // were removed or the bounds became larger).
            let mut scroll_offset = scroll_offset.borrow_mut();
            scroll_offset.x = scroll_offset.x.clamp(-scroll_max.width, px(0.));
            scroll_offset.y = scroll_offset.y.clamp(-scroll_max.height, px(0.));
            *scroll_offset
        } else {
            Point::default()
        }
    }

    /// Paint this element according to this interactivity state's configured styles
    /// and bind the element's mouse and keyboard events.
    ///
    /// content_size is the size of the content of the element, which may be larger than the
    /// element's bounds if the element is scrollable.
    ///
    /// the final computed style will be passed to the provided function, along
    /// with the current scroll offset
    pub fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        hitbox: Option<&Hitbox>,
        window: &mut Window,
        cx: &mut AppContext,
        f: impl FnOnce(&Style, &mut Window, &mut AppContext),
    ) {
        self.hovered = hitbox.map(|hitbox| hitbox.is_hovered(window));
        window.with_optional_element_state::<InteractiveElementState, _>(
            global_id,
            |element_state, window| {
                let mut element_state =
                    element_state.map(|element_state| element_state.unwrap_or_default());

                let style = self.compute_style_internal(hitbox, element_state.as_mut(), window, cx);

                #[cfg(any(feature = "test-support", test))]
                if let Some(debug_selector) = &self.debug_selector {
                    window
                        .next_frame
                        .debug_bounds
                        .insert(debug_selector.clone(), bounds);
                }

                self.paint_hover_group_handler(window, cx);

                if style.visibility == Visibility::Hidden {
                    return ((), element_state);
                }

                window.with_element_opacity(style.opacity, |window| {
                    style.paint(
                        bounds,
                        window,
                        cx,
                        |window: &mut Window, cx: &mut AppContext| {
                            window.with_text_style(style.text_style().cloned(), |window| {
                                window.with_content_mask(
                                    style.overflow_mask(bounds, window.rem_size()),
                                    |window| {
                                        if let Some(hitbox) = hitbox {
                                            #[cfg(debug_assertions)]
                                            self.paint_debug_info(
                                                global_id, hitbox, &style, window, cx,
                                            );

                                            if !cx.has_active_drag() {
                                                if let Some(mouse_cursor) = style.mouse_cursor {
                                                    window.set_cursor_style(mouse_cursor, hitbox);
                                                }
                                            }

                                            if let Some(group) = self.group.clone() {
                                                GroupHitboxes::push(group, hitbox.id, cx);
                                            }

                                            self.paint_mouse_listeners(
                                                hitbox,
                                                element_state.as_mut(),
                                                window,
                                                cx,
                                            );
                                            self.paint_scroll_listener(hitbox, &style, window, cx);
                                        }

                                        self.paint_keyboard_listeners(window, cx);
                                        f(&style, window, cx);

                                        if hitbox.is_some() {
                                            if let Some(group) = self.group.as_ref() {
                                                GroupHitboxes::pop(group, cx);
                                            }
                                        }
                                    },
                                );
                            });
                        },
                    );
                });

                ((), element_state)
            },
        );
    }

    #[cfg(debug_assertions)]
    fn paint_debug_info(
        &self,
        global_id: Option<&GlobalElementId>,
        hitbox: &Hitbox,
        style: &Style,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        if global_id.is_some()
            && (style.debug || style.debug_below || cx.has_global::<crate::DebugBelow>())
            && hitbox.is_hovered(window)
        {
            const FONT_SIZE: crate::Pixels = crate::Pixels(10.);
            let element_id = format!("{:?}", global_id.unwrap());
            let str_len = element_id.len();

            let render_debug_text = |window: &mut Window, cx: &mut AppContext| {
                if let Some(text) = window
                    .text_system()
                    .shape_text(
                        element_id.into(),
                        FONT_SIZE,
                        &[window.text_style().to_run(str_len)],
                        None,
                    )
                    .ok()
                    .and_then(|mut text| text.pop())
                {
                    text.paint(hitbox.origin, FONT_SIZE, window, cx).ok();

                    let text_bounds = crate::Bounds {
                        origin: hitbox.origin,
                        size: text.size(FONT_SIZE),
                    };
                    if self.location.is_some()
                        && text_bounds.contains(&window.mouse_position())
                        && window.modifiers().secondary()
                    {
                        let secondary_held = window.modifiers().secondary();
                        window.on_key_event({
                            move |e: &crate::ModifiersChangedEvent, _phase, window, cx| {
                                if e.modifiers.secondary() != secondary_held
                                    && text_bounds.contains(&window.mouse_position())
                                {
                                    cx.refresh();
                                }
                            }
                        });

                        let was_hovered = hitbox.is_hovered(window);
                        window.on_mouse_event({
                            let hitbox = hitbox.clone();
                            move |_: &MouseMoveEvent, phase, window, cx| {
                                if phase == DispatchPhase::Capture {
                                    let hovered = hitbox.is_hovered(window);
                                    if hovered != was_hovered {
                                        cx.refresh();
                                    }
                                }
                            }
                        });

                        window.on_mouse_event({
                            let hitbox = hitbox.clone();
                            let location = self.location.unwrap();
                            move |e: &crate::MouseDownEvent, phase, window, cx| {
                                if text_bounds.contains(&e.position)
                                    && phase.capture()
                                    && hitbox.is_hovered(window)
                                {
                                    cx.stop_propagation();
                                    let Ok(dir) = std::env::current_dir() else {
                                        return;
                                    };

                                    eprintln!(
                                        "This element was created at:\n{}:{}:{}",
                                        dir.join(location.file()).to_string_lossy(),
                                        location.line(),
                                        location.column()
                                    );
                                }
                            }
                        });
                        window.paint_quad(crate::outline(
                            crate::Bounds {
                                origin: hitbox.origin
                                    + crate::point(crate::px(0.), FONT_SIZE - px(2.)),
                                size: crate::Size {
                                    width: text_bounds.size.width,
                                    height: crate::px(1.),
                                },
                            },
                            crate::red(),
                        ))
                    }
                }
            };

            window.with_text_style(
                Some(crate::TextStyleRefinement {
                    color: Some(crate::red()),
                    line_height: Some(FONT_SIZE.into()),
                    background_color: Some(crate::white()),
                    ..Default::default()
                }),
                |window| render_debug_text(window, cx),
            )
        }
    }

    fn paint_mouse_listeners(
        &mut self,
        hitbox: &Hitbox,
        element_state: Option<&mut InteractiveElementState>,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        // If this element can be focused, register a mouse down listener
        // that will automatically transfer focus when hitting the element.
        // This behavior can be suppressed by using `cx.prevent_default()`.
        if let Some(focus_handle) = self.tracked_focus_handle.clone() {
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && hitbox.is_hovered(window)
                    && !window.default_prevented()
                {
                    window.focus(&focus_handle);
                    // If there is a parent that is also focusable, prevent it
                    // from transferring focus because we already did so.
                    window.prevent_default();
                }
            });
        }

        for listener in self.mouse_down_listeners.drain(..) {
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                listener(event, phase, &hitbox, window, cx);
            })
        }

        for listener in self.mouse_up_listeners.drain(..) {
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                listener(event, phase, &hitbox, window, cx);
            })
        }

        for listener in self.mouse_move_listeners.drain(..) {
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                listener(event, phase, &hitbox, window, cx);
            })
        }

        for listener in self.scroll_wheel_listeners.drain(..) {
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                listener(event, phase, &hitbox, window, cx);
            })
        }

        if self.hover_style.is_some()
            || self.base_style.mouse_cursor.is_some()
            || cx.active_drag.is_some() && !self.drag_over_styles.is_empty()
        {
            let hitbox = hitbox.clone();
            let was_hovered = hitbox.is_hovered(window);
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                let hovered = hitbox.is_hovered(window);
                if phase == DispatchPhase::Capture && hovered != was_hovered {
                    cx.refresh();
                }
            });
        }

        let mut drag_listener = mem::take(&mut self.drag_listener);
        let drop_listeners = mem::take(&mut self.drop_listeners);
        let click_listeners = mem::take(&mut self.click_listeners);
        let can_drop_predicate = mem::take(&mut self.can_drop_predicate);

        if !drop_listeners.is_empty() {
            let hitbox = hitbox.clone();
            window.on_mouse_event({
                move |_: &MouseUpEvent, phase, window, cx| {
                    if let Some(drag) = &cx.active_drag {
                        if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                            let drag_state_type = drag.value.as_ref().type_id();
                            for (drop_state_type, listener) in &drop_listeners {
                                if *drop_state_type == drag_state_type {
                                    let drag = cx
                                        .active_drag
                                        .take()
                                        .expect("checked for type drag state type above");

                                    let mut can_drop = true;
                                    if let Some(predicate) = &can_drop_predicate {
                                        can_drop = predicate(drag.value.as_ref(), window, cx);
                                    }

                                    if can_drop {
                                        listener(drag.value.as_ref(), window, cx);
                                        cx.refresh();
                                        cx.stop_propagation();
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        if let Some(element_state) = element_state {
            if !click_listeners.is_empty() || drag_listener.is_some() {
                let pending_mouse_down = element_state
                    .pending_mouse_down
                    .get_or_insert_with(Default::default)
                    .clone();

                let clicked_state = element_state
                    .clicked_state
                    .get_or_insert_with(Default::default)
                    .clone();

                window.on_mouse_event({
                    let pending_mouse_down = pending_mouse_down.clone();
                    let hitbox = hitbox.clone();
                    move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.button == MouseButton::Left
                            && hitbox.is_hovered(window)
                        {
                            *pending_mouse_down.borrow_mut() = Some(event.clone());
                            cx.refresh();
                        }
                    }
                });

                window.on_mouse_event({
                    let pending_mouse_down = pending_mouse_down.clone();
                    let hitbox = hitbox.clone();
                    move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            return;
                        }

                        let mut pending_mouse_down = pending_mouse_down.borrow_mut();
                        if let Some(mouse_down) = pending_mouse_down.clone() {
                            if !cx.has_active_drag()
                                && (event.position - mouse_down.position).magnitude()
                                    > DRAG_THRESHOLD
                            {
                                if let Some((drag_value, drag_listener)) = drag_listener.take() {
                                    *clicked_state.borrow_mut() = ElementClickedState::default();
                                    let cursor_offset = event.position - hitbox.origin;
                                    let render_drag = (drag_listener)(
                                        drag_value.as_ref(),
                                        cursor_offset,
                                        window,
                                        cx,
                                    );
                                    cx.active_drag = Some(AnyDrag {
                                        render: Box::new(move |value, window, cx| {
                                            render_drag(value, cursor_offset, window, cx)
                                        }),
                                        value: drag_value,
                                        cursor_offset,
                                    });
                                    pending_mouse_down.take();
                                    cx.refresh();
                                    cx.stop_propagation();
                                }
                            }
                        }
                    }
                });

                window.on_mouse_event({
                    let mut captured_mouse_down = None;
                    let hitbox = hitbox.clone();
                    move |event: &MouseUpEvent, phase, window, cx| match phase {
                        // Clear the pending mouse down during the capture phase,
                        // so that it happens even if another event handler stops
                        // propagation.
                        DispatchPhase::Capture => {
                            let mut pending_mouse_down = pending_mouse_down.borrow_mut();
                            if pending_mouse_down.is_some() && hitbox.is_hovered(window) {
                                captured_mouse_down = pending_mouse_down.take();
                                cx.refresh();
                            }
                        }
                        // Fire click handlers during the bubble phase.
                        DispatchPhase::Bubble => {
                            if let Some(mouse_down) = captured_mouse_down.take() {
                                let mouse_click = ClickEvent {
                                    down: mouse_down,
                                    up: event.clone(),
                                };
                                for listener in &click_listeners {
                                    listener(&mouse_click, window, cx);
                                }
                            }
                        }
                    }
                });
            }

            if let Some(hover_listener) = self.hover_listener.take() {
                let hitbox = hitbox.clone();
                let was_hovered = element_state
                    .hover_state
                    .get_or_insert_with(Default::default)
                    .clone();
                let has_mouse_down = element_state
                    .pending_mouse_down
                    .get_or_insert_with(Default::default)
                    .clone();

                window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let is_hovered = has_mouse_down.borrow().is_none()
                        && !cx.has_active_drag()
                        && hitbox.is_hovered(window);
                    let mut was_hovered = was_hovered.borrow_mut();

                    if is_hovered != *was_hovered {
                        *was_hovered = is_hovered;
                        drop(was_hovered);

                        hover_listener(&is_hovered, window, cx);
                    }
                });
            }

            // Ensure to remove active tooltip if tooltip builder is none
            if self.tooltip_builder.is_none() {
                element_state.active_tooltip.take();
            }

            if let Some(tooltip_builder) = self.tooltip_builder.take() {
                let tooltip_is_hoverable = tooltip_builder.hoverable;
                let active_tooltip = element_state
                    .active_tooltip
                    .get_or_insert_with(Default::default)
                    .clone();
                let pending_mouse_down = element_state
                    .pending_mouse_down
                    .get_or_insert_with(Default::default)
                    .clone();

                window.on_mouse_event({
                    let active_tooltip = active_tooltip.clone();
                    let hitbox = hitbox.clone();
                    let tooltip_id = self.tooltip_id;
                    move |_: &MouseMoveEvent, phase, window, cx| {
                        let is_hovered =
                            pending_mouse_down.borrow().is_none() && hitbox.is_hovered(window);
                        let tooltip_is_hovered =
                            tooltip_id.map_or(false, |tooltip_id| tooltip_id.is_hovered(window));
                        if !is_hovered && (!tooltip_is_hoverable || !tooltip_is_hovered) {
                            if active_tooltip.borrow_mut().take().is_some() {
                                cx.refresh();
                            }

                            return;
                        }

                        if phase != DispatchPhase::Bubble {
                            return;
                        }

                        if active_tooltip.borrow().is_none() {
                            let task = cx.spawn({
                                let active_tooltip = active_tooltip.clone();
                                let build_tooltip = tooltip_builder.build.clone();
                                let window_handle = window.window_handle();
                                move |mut cx| async move {
                                    cx.background_executor().timer(TOOLTIP_DELAY).await;
                                    window_handle
                                        .update(&mut cx, |window, cx| {
                                            active_tooltip.borrow_mut().replace(ActiveTooltip {
                                                tooltip: Some(AnyTooltip {
                                                    render: build_tooltip(window, cx),
                                                    mouse_position: window.mouse_position(),
                                                }),
                                                _task: None,
                                            });
                                            cx.refresh();
                                        })
                                        .ok();
                                }
                            });
                            active_tooltip.borrow_mut().replace(ActiveTooltip {
                                tooltip: None,
                                _task: Some(task),
                            });
                        }
                    }
                });

                window.on_mouse_event({
                    let active_tooltip = active_tooltip.clone();
                    let tooltip_id = self.tooltip_id;
                    move |_: &MouseDownEvent, _, window, cx| {
                        let tooltip_is_hovered =
                            tooltip_id.map_or(false, |tooltip_id| tooltip_id.is_hovered(window));

                        if (!tooltip_is_hoverable || !tooltip_is_hovered)
                            && active_tooltip.borrow_mut().take().is_some()
                        {
                            cx.refresh();
                        }
                    }
                });

                window.on_mouse_event({
                    let active_tooltip = active_tooltip.clone();
                    let tooltip_id = self.tooltip_id;
                    move |_: &ScrollWheelEvent, _, window, cx| {
                        let tooltip_is_hovered =
                            tooltip_id.map_or(false, |tooltip_id| tooltip_id.is_hovered(window));
                        if (!tooltip_is_hoverable || !tooltip_is_hovered)
                            && active_tooltip.borrow_mut().take().is_some()
                        {
                            cx.refresh();
                        }
                    }
                })
            }

            let active_state = element_state
                .clicked_state
                .get_or_insert_with(Default::default)
                .clone();
            if active_state.borrow().is_clicked() {
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        *active_state.borrow_mut() = ElementClickedState::default();
                        cx.refresh();
                    }
                });
            } else {
                let active_group_hitbox = self
                    .group_active_style
                    .as_ref()
                    .and_then(|group_active| GroupHitboxes::get(&group_active.group, cx));
                let hitbox = hitbox.clone();
                window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && !window.default_prevented() {
                        let group_hovered = active_group_hitbox
                            .map_or(false, |group_hitbox_id| group_hitbox_id.is_hovered(window));
                        let element_hovered = hitbox.is_hovered(window);
                        if group_hovered || element_hovered {
                            *active_state.borrow_mut() = ElementClickedState {
                                group: group_hovered,
                                element: element_hovered,
                            };
                            cx.refresh();
                        }
                    }
                });
            }
        }
    }

    fn paint_keyboard_listeners(&mut self, window: &mut Window, cx: &mut AppContext) {
        let key_down_listeners = mem::take(&mut self.key_down_listeners);
        let key_up_listeners = mem::take(&mut self.key_up_listeners);
        let modifiers_changed_listeners = mem::take(&mut self.modifiers_changed_listeners);
        let action_listeners = mem::take(&mut self.action_listeners);
        if let Some(context) = self.key_context.clone() {
            window.set_key_context(context);
        }

        for listener in key_down_listeners {
            window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
                listener(event, phase, window, cx);
            })
        }

        for listener in key_up_listeners {
            window.on_key_event(move |event: &KeyUpEvent, phase, window, cx| {
                listener(event, phase, window, cx);
            })
        }

        for listener in modifiers_changed_listeners {
            window.on_modifiers_changed(move |event: &ModifiersChangedEvent, window, cx| {
                listener(event, window, cx);
            })
        }

        for (action_type, listener) in action_listeners {
            window.on_action(action_type, listener)
        }
    }

    fn paint_hover_group_handler(&self, window: &mut Window, cx: &mut AppContext) {
        let group_hitbox = self
            .group_hover_style
            .as_ref()
            .and_then(|group_hover| GroupHitboxes::get(&group_hover.group, cx));

        if let Some(group_hitbox) = group_hitbox {
            let was_hovered = group_hitbox.is_hovered(window);
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                let hovered = group_hitbox.is_hovered(window);
                if phase == DispatchPhase::Capture && hovered != was_hovered {
                    cx.refresh();
                }
            });
        }
    }

    fn paint_scroll_listener(
        &self,
        hitbox: &Hitbox,
        style: &Style,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        if let Some(scroll_offset) = self.scroll_offset.clone() {
            let overflow = style.overflow;
            let allow_concurrent_scroll = style.allow_concurrent_scroll;
            let line_height = window.line_height();
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    let mut scroll_offset = scroll_offset.borrow_mut();
                    let old_scroll_offset = *scroll_offset;
                    let delta = event.delta.pixel_delta(line_height);

                    let mut delta_x = Pixels::ZERO;
                    if overflow.x == Overflow::Scroll {
                        if !delta.x.is_zero() {
                            delta_x = delta.x;
                        } else if overflow.y != Overflow::Scroll {
                            delta_x = delta.y;
                        }
                    }
                    let mut delta_y = Pixels::ZERO;
                    if overflow.y == Overflow::Scroll {
                        if !delta.y.is_zero() {
                            delta_y = delta.y;
                        } else if overflow.x != Overflow::Scroll {
                            delta_y = delta.x;
                        }
                    }
                    if !allow_concurrent_scroll && !delta_x.is_zero() && !delta_y.is_zero() {
                        if delta_x.abs() > delta_y.abs() {
                            delta_y = Pixels::ZERO;
                        } else {
                            delta_x = Pixels::ZERO;
                        }
                    }
                    scroll_offset.y += delta_y;
                    scroll_offset.x += delta_x;
                    cx.stop_propagation();
                    if *scroll_offset != old_scroll_offset {
                        cx.refresh();
                    }
                }
            });
        }
    }

    /// Compute the visual style for this element, based on the current bounds and the element's state.
    pub fn compute_style(
        &self,
        global_id: Option<&GlobalElementId>,
        hitbox: Option<&Hitbox>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Style {
        window.with_optional_element_state(global_id, |element_state, window| {
            let mut element_state =
                element_state.map(|element_state| element_state.unwrap_or_default());
            let style = self.compute_style_internal(hitbox, element_state.as_mut(), window, cx);
            (style, element_state)
        })
    }

    /// Called from internal methods that have already called with_element_state.
    fn compute_style_internal(
        &self,
        hitbox: Option<&Hitbox>,
        element_state: Option<&mut InteractiveElementState>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Style {
        let mut style = Style::default();
        style.refine(&self.base_style);

        if let Some(focus_handle) = self.tracked_focus_handle.as_ref() {
            if let Some(in_focus_style) = self.in_focus_style.as_ref() {
                if focus_handle.within_focused(window) {
                    style.refine(in_focus_style);
                }
            }

            if let Some(focus_style) = self.focus_style.as_ref() {
                if focus_handle.is_focused(window) {
                    style.refine(focus_style);
                }
            }
        }

        if let Some(hitbox) = hitbox {
            if !cx.has_active_drag() {
                if let Some(group_hover) = self.group_hover_style.as_ref() {
                    if let Some(group_hitbox_id) = GroupHitboxes::get(&group_hover.group, cx) {
                        if group_hitbox_id.is_hovered(window) {
                            style.refine(&group_hover.style);
                        }
                    }
                }

                if let Some(hover_style) = self.hover_style.as_ref() {
                    if hitbox.is_hovered(window) {
                        style.refine(hover_style);
                    }
                }
            }

            if let Some(drag) = cx.active_drag.take() {
                let mut can_drop = true;
                if let Some(can_drop_predicate) = &self.can_drop_predicate {
                    can_drop = can_drop_predicate(drag.value.as_ref(), window, cx);
                }

                if can_drop {
                    for (state_type, group_drag_style) in &self.group_drag_over_styles {
                        if let Some(group_hitbox_id) =
                            GroupHitboxes::get(&group_drag_style.group, cx)
                        {
                            if *state_type == drag.value.as_ref().type_id()
                                && group_hitbox_id.is_hovered(window)
                            {
                                style.refine(&group_drag_style.style);
                            }
                        }
                    }

                    for (state_type, build_drag_over_style) in &self.drag_over_styles {
                        if *state_type == drag.value.as_ref().type_id() && hitbox.is_hovered(window)
                        {
                            style.refine(&build_drag_over_style(drag.value.as_ref(), window, cx));
                        }
                    }
                }

                cx.active_drag = Some(drag);
            }
        }

        if let Some(element_state) = element_state {
            let clicked_state = element_state
                .clicked_state
                .get_or_insert_with(Default::default)
                .borrow();
            if clicked_state.group {
                if let Some(group) = self.group_active_style.as_ref() {
                    style.refine(&group.style)
                }
            }

            if let Some(active_style) = self.active_style.as_ref() {
                if clicked_state.element {
                    style.refine(active_style)
                }
            }
        }

        style
    }
}

/// The per-frame state of an interactive element. Used for tracking stateful interactions like clicks
/// and scroll offsets.
#[derive(Default)]
pub struct InteractiveElementState {
    pub(crate) focus_handle: Option<FocusHandle>,
    pub(crate) clicked_state: Option<Rc<RefCell<ElementClickedState>>>,
    pub(crate) hover_state: Option<Rc<RefCell<bool>>>,
    pub(crate) pending_mouse_down: Option<Rc<RefCell<Option<MouseDownEvent>>>>,
    pub(crate) scroll_offset: Option<Rc<RefCell<Point<Pixels>>>>,
    pub(crate) active_tooltip: Option<Rc<RefCell<Option<ActiveTooltip>>>>,
}

/// The current active tooltip
pub struct ActiveTooltip {
    pub(crate) tooltip: Option<AnyTooltip>,
    pub(crate) _task: Option<Task<()>>,
}

/// Whether or not the element or a group that contains it is clicked by the mouse.
#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct ElementClickedState {
    /// True if this element's group has been clicked, false otherwise
    pub group: bool,

    /// True if this element has been clicked, false otherwise
    pub element: bool,
}

impl ElementClickedState {
    fn is_clicked(&self) -> bool {
        self.group || self.element
    }
}

#[derive(Default)]
pub(crate) struct GroupHitboxes(HashMap<SharedString, SmallVec<[HitboxId; 1]>>);

impl Global for GroupHitboxes {}

impl GroupHitboxes {
    pub fn get(name: &SharedString, cx: &mut AppContext) -> Option<HitboxId> {
        cx.default_global::<Self>()
            .0
            .get(name)
            .and_then(|bounds_stack| bounds_stack.last())
            .cloned()
    }

    pub fn push(name: SharedString, hitbox_id: HitboxId, cx: &mut AppContext) {
        cx.default_global::<Self>()
            .0
            .entry(name)
            .or_default()
            .push(hitbox_id);
    }

    pub fn pop(name: &SharedString, cx: &mut AppContext) {
        cx.default_global::<Self>().0.get_mut(name).unwrap().pop();
    }
}

/// A wrapper around an element that can be focused.
pub struct Focusable<E> {
    /// The element that is focusable
    pub element: E,
}

impl<E: InteractiveElement> FocusableElement for Focusable<E> {}

impl<E> InteractiveElement for Focusable<E>
where
    E: InteractiveElement,
{
    fn interactivity(&mut self) -> &mut Interactivity {
        self.element.interactivity()
    }
}

impl<E: StatefulInteractiveElement> StatefulInteractiveElement for Focusable<E> {}

impl<E> Styled for Focusable<E>
where
    E: Styled,
{
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl<E> Element for Focusable<E>
where
    E: Element,
{
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.element.id()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.element.request_layout(id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> E::PrepaintState {
        self.element.prepaint(id, bounds, state, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.element
            .paint(id, bounds, request_layout, prepaint, window, cx)
    }
}

impl<E> IntoElement for Focusable<E>
where
    E: IntoElement,
{
    type Element = E::Element;

    fn into_element(self) -> Self::Element {
        self.element.into_element()
    }
}

impl<E> ParentElement for Focusable<E>
where
    E: ParentElement,
{
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.element.extend(elements)
    }
}

/// A wrapper around an element that can store state, produced after assigning an ElementId.
pub struct Stateful<E> {
    pub(crate) element: E,
}

impl<E> Styled for Stateful<E>
where
    E: Styled,
{
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl<E> StatefulInteractiveElement for Stateful<E>
where
    E: Element,
    Self: InteractiveElement,
{
}

impl<E> InteractiveElement for Stateful<E>
where
    E: InteractiveElement,
{
    fn interactivity(&mut self) -> &mut Interactivity {
        self.element.interactivity()
    }
}

impl<E: FocusableElement> FocusableElement for Stateful<E> {}

impl<E> Element for Stateful<E>
where
    E: Element,
{
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.element.id()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.element.request_layout(id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> E::PrepaintState {
        self.element.prepaint(id, bounds, state, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.element
            .paint(id, bounds, request_layout, prepaint, window, cx);
    }
}

impl<E> IntoElement for Stateful<E>
where
    E: Element,
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E> ParentElement for Stateful<E>
where
    E: ParentElement,
{
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.element.extend(elements)
    }
}

/// Represents an element that can be scrolled *to* in its parent element.
///
/// Contrary to [ScrollHandle::scroll_to_item], an anchored element does not have to be an immediate child of the parent.
#[derive(Clone)]
pub struct ScrollAnchor {
    handle: ScrollHandle,
    last_origin: Rc<RefCell<Point<Pixels>>>,
}

impl ScrollAnchor {
    /// Creates a [ScrollAnchor] associated with a given [ScrollHandle].
    pub fn for_handle(handle: ScrollHandle) -> Self {
        Self {
            handle,
            last_origin: Default::default(),
        }
    }
    /// Request scroll to this item on the next frame.
    pub fn scroll_to(&self, window: &mut Window) {
        let this = self.clone();

        window.on_next_frame(move |_, _| {
            let viewport_bounds = this.handle.bounds();
            let self_bounds = *this.last_origin.borrow();
            this.handle.set_offset(viewport_bounds.origin - self_bounds);
        });
    }
}
#[derive(Default, Debug)]
struct ScrollHandleState {
    offset: Rc<RefCell<Point<Pixels>>>,
    bounds: Bounds<Pixels>,
    child_bounds: Vec<Bounds<Pixels>>,
    requested_scroll_top: Option<(usize, Pixels)>,
    overflow: Point<Overflow>,
}

/// A handle to the scrollable aspects of an element.
/// Used for accessing scroll state, like the current scroll offset,
/// and for mutating the scroll state, like scrolling to a specific child.
#[derive(Clone, Debug)]
pub struct ScrollHandle(Rc<RefCell<ScrollHandleState>>);

impl Default for ScrollHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollHandle {
    /// Construct a new scroll handle.
    pub fn new() -> Self {
        Self(Rc::default())
    }

    /// Get the current scroll offset.
    pub fn offset(&self) -> Point<Pixels> {
        *self.0.borrow().offset.borrow()
    }

    /// Get the top child that's scrolled into view.
    pub fn top_item(&self) -> usize {
        let state = self.0.borrow();
        let top = state.bounds.top() - state.offset.borrow().y;

        match state.child_bounds.binary_search_by(|bounds| {
            if top < bounds.top() {
                Ordering::Greater
            } else if top > bounds.bottom() {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        }) {
            Ok(ix) => ix,
            Err(ix) => ix.min(state.child_bounds.len().saturating_sub(1)),
        }
    }

    /// Return the bounds into which this child is painted
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().bounds
    }

    /// Set the bounds into which this child is painted
    pub(super) fn set_bounds(&self, bounds: Bounds<Pixels>) {
        self.0.borrow_mut().bounds = bounds;
    }

    /// Get the bounds for a specific child.
    pub fn bounds_for_item(&self, ix: usize) -> Option<Bounds<Pixels>> {
        self.0.borrow().child_bounds.get(ix).cloned()
    }

    /// scroll_to_item scrolls the minimal amount to ensure that the child is
    /// fully visible
    pub fn scroll_to_item(&self, ix: usize) {
        let state = self.0.borrow();

        let Some(bounds) = state.child_bounds.get(ix) else {
            return;
        };

        let mut scroll_offset = state.offset.borrow_mut();

        if state.overflow.y == Overflow::Scroll {
            if bounds.top() + scroll_offset.y < state.bounds.top() {
                scroll_offset.y = state.bounds.top() - bounds.top();
            } else if bounds.bottom() + scroll_offset.y > state.bounds.bottom() {
                scroll_offset.y = state.bounds.bottom() - bounds.bottom();
            }
        }

        if state.overflow.x == Overflow::Scroll {
            if bounds.left() + scroll_offset.x < state.bounds.left() {
                scroll_offset.x = state.bounds.left() - bounds.left();
            } else if bounds.right() + scroll_offset.x > state.bounds.right() {
                scroll_offset.x = state.bounds.right() - bounds.right();
            }
        }
    }

    /// Set the offset explicitly. The offset is the distance from the top left of the
    /// parent container to the top left of the first child.
    /// As you scroll further down the offset becomes more negative.
    pub fn set_offset(&self, mut position: Point<Pixels>) {
        let state = self.0.borrow();
        *state.offset.borrow_mut() = position;
    }

    /// Get the logical scroll top, based on a child index and a pixel offset.
    pub fn logical_scroll_top(&self) -> (usize, Pixels) {
        let ix = self.top_item();
        let state = self.0.borrow();

        if let Some(child_bounds) = state.child_bounds.get(ix) {
            (
                ix,
                child_bounds.top() + state.offset.borrow().y - state.bounds.top(),
            )
        } else {
            (ix, px(0.))
        }
    }

    /// Set the logical scroll top, based on a child index and a pixel offset.
    pub fn set_logical_scroll_top(&self, ix: usize, px: Pixels) {
        self.0.borrow_mut().requested_scroll_top = Some((ix, px));
    }

    /// Get the count of children for scrollable item.
    pub fn children_count(&self) -> usize {
        self.0.borrow().child_bounds.len()
    }
}