use crate::{
    AppContext, Bounds, InputHandler, Model, ModelContext, Pixels, UTF16Selection, Window,
};
use std::ops::Range;

/// Implement this trait to allow models to handle textual input when implementing an editor, field, etc.
///
/// Once your model implements this trait, you can use it to construct an [`ElementInputHandler<V>`].
/// This input handler can then be assigned during paint by calling [`WindowContext::handle_input`].
///
/// See [`InputHandler`] for details on how to implement each method.
pub trait InputHandlerModel: 'static + Sized {
    /// See [`InputHandler::text_for_range`] for details
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut ModelContext<Self>,
    ) -> Option<String>;

    /// See [`InputHandler::selected_text_range`] for details
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut ModelContext<Self>,
    ) -> Option<UTF16Selection>;

    /// See [`InputHandler::marked_text_range`] for details
    fn marked_text_range(
        &mut self,
        window: &mut Window,
        cx: &mut ModelContext<Self>,
    ) -> Option<Range<usize>>;

    /// See [`InputHandler::unmark_text`] for details
    fn unmark_text(&mut self, window: &mut Window, cx: &mut ModelContext<Self>);

    /// See [`InputHandler::replace_text_in_range`] for details
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut ModelContext<Self>,
    );

    /// See [`InputHandler::replace_and_mark_text_in_range`] for details
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut ModelContext<Self>,
    );

    /// See [`InputHandler::bounds_for_range`] for details
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut ModelContext<Self>,
    ) -> Option<Bounds<Pixels>>;
}

impl<T: InputHandlerModel> InputHandler for Model<T> {
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<UTF16Selection> {
        self.update(cx, |model, cx| {
            model.selected_text_range(ignore_disabled_input, window, cx)
        })
    }

    fn marked_text_range(
        &mut self,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<Range<usize>> {
        self.update(cx, |model, cx| model.marked_text_range(window, cx))
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<String> {
        self.update(cx, |model, cx| {
            model.text_for_range(range_utf16, adjusted_range, window, cx)
        })
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.update(cx, |model, cx| {
            model.replace_text_in_range(replacement_range, text, window, cx)
        })
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.update(cx, |model, cx| {
            model.replace_and_mark_text_in_range(
                range_utf16,
                new_text,
                new_selected_range,
                window,
                cx,
            )
        })
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut AppContext) {
        self.update(cx, |model, cx| model.unmark_text(window, cx))
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<Bounds<Pixels>> {
        self.update(cx, |model, cx| {
            model.bounds_for_range(range_utf16, Bounds::default(), window, cx)
        })
    }
}

/// The canonical implementation of [`PlatformInputHandler`]. Call [`WindowContext::handle_input`]
/// with an instance during your element's paint.
pub struct ElementInputHandler<T> {
    model: Model<T>,
    element_bounds: Bounds<Pixels>,
}

impl<T: 'static> ElementInputHandler<T> {
    /// Used in [`Element::paint`][element_paint] with the element's bounds and a model context for its
    /// containing model.
    ///
    /// [element_paint]: crate::Element::paint
    pub fn new(element_bounds: Bounds<Pixels>, model: Model<T>) -> Self {
        ElementInputHandler {
            model,
            element_bounds,
        }
    }
}

impl<V: InputHandlerModel> InputHandler for ElementInputHandler<V> {
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<UTF16Selection> {
        self.model.update(cx, |model, cx| {
            model.selected_text_range(ignore_disabled_input, window, cx)
        })
    }

    fn marked_text_range(
        &mut self,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<Range<usize>> {
        self.model
            .update(cx, |model, cx| model.marked_text_range(window, cx))
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<String> {
        self.model.update(cx, |model, cx| {
            model.text_for_range(range_utf16, adjusted_range, window, cx)
        })
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.model.update(cx, |model, cx| {
            model.replace_text_in_range(replacement_range, text, window, cx)
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut AppContext,
    ) {
        self.model.update(cx, |model, cx| {
            model.replace_and_mark_text_in_range(
                range_utf16,
                new_text,
                new_selected_range,
                window,
                cx,
            )
        });
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut AppContext) {
        self.model
            .update(cx, |model, cx| model.unmark_text(window, cx));
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut AppContext,
    ) -> Option<Bounds<Pixels>> {
        self.model.update(cx, |model, cx| {
            model.bounds_for_range(range_utf16, self.element_bounds, window, cx)
        })
    }
}
