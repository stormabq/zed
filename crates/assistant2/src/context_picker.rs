use std::sync::Arc;

use gpui::{DismissEvent, SharedString, Task, WeakView};
use picker::{Picker, PickerDelegate, PickerEditorPosition};
use ui::{prelude::*, ListItem, ListItemSpacing, PopoverMenu, PopoverTrigger, Tooltip};

use crate::message_editor::MessageEditor;

#[derive(IntoElement)]
pub(super) struct ContextPicker<T: PopoverTrigger> {
    message_editor: WeakModel<MessageEditor>,
    trigger: T,
}

#[derive(Clone)]
struct ContextPickerEntry {
    name: SharedString,
    description: SharedString,
    icon: IconName,
}

pub(crate) struct ContextPickerDelegate {
    all_entries: Vec<ContextPickerEntry>,
    filtered_entries: Vec<ContextPickerEntry>,
    message_editor: WeakModel<MessageEditor>,
    selected_ix: usize,
}

impl<T: PopoverTrigger> ContextPicker<T> {
    pub(crate) fn new(message_editor: WeakModel<MessageEditor>, trigger: T) -> Self {
        ContextPicker {
            message_editor,
            trigger,
        }
    }
}

impl PickerDelegate for ContextPickerDelegate {
    type ListItem = ListItem;

    fn match_count(&self) -> usize {
        self.filtered_entries.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_ix
    }

    fn set_selected_index(&mut self, ix: usize, model: &Model<Picker>, cx: &mut AppContext) {
        self.selected_ix = ix.min(self.filtered_entries.len().saturating_sub(1));
        model.notify(cx);
    }

    fn placeholder_text(&self, _window: &mut gpui::Window, _cx: &mut gpui::AppContext) -> Arc<str> {
        "Select a context source…".into()
    }

    fn update_matches(
        &mut self,
        query: String,
        model: &Model<Picker>,
        cx: &mut AppContext,
    ) -> Task<()> {
        let all_commands = self.all_entries.clone();
        model.spawn(cx, |this, mut cx| async move {
            let filtered_commands = cx
                .background_executor()
                .spawn(async move {
                    if query.is_empty() {
                        all_commands
                    } else {
                        all_commands
                            .into_iter()
                            .filter(|model_info| {
                                model_info
                                    .name
                                    .to_lowercase()
                                    .contains(&query.to_lowercase())
                            })
                            .collect()
                    }
                })
                .await;

            this.update(&mut cx, |this, model, cx| {
                this.delegate.filtered_entries = filtered_commands;
                this.delegate.set_selected_index(0, cx);
                model.notify(cx);
            })
            .ok();
        })
    }

    fn confirm(&mut self, _secondary: bool, model: &Model<Picker>, cx: &mut AppContext) {
        if let Some(entry) = self.filtered_entries.get(self.selected_ix) {
            self.message_editor
                .update(cx, |_message_editor, model, _cx| {
                    println!("Insert context from {}", entry.name);
                })
                .ok();
            model.emit(DismissEvent, cx);
        }
    }

    fn dismissed(&mut self, model: &Model<Picker>, _cx: &mut AppContext) {}

    fn editor_position(&self) -> PickerEditorPosition {
        PickerEditorPosition::End
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        model: &Model<Picker>,
        _cx: &mut AppContext,
    ) -> Option<Self::ListItem> {
        let entry = self.filtered_entries.get(ix)?;

        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Dense)
                .selected(selected)
                .tooltip({
                    let description = entry.description.clone();
                    move |cx| {
                        cx.new_model(|_model, _cx| Tooltip::new(description.clone()))
                            .into()
                    }
                })
                .child(
                    v_flex()
                        .group(format!("context-entry-label-{ix}"))
                        .w_full()
                        .py_0p5()
                        .min_w(px(250.))
                        .max_w(px(400.))
                        .child(
                            h_flex()
                                .gap_1p5()
                                .child(Icon::new(entry.icon).size(IconSize::XSmall))
                                .child(
                                    Label::new(entry.name.clone())
                                        .single_line()
                                        .size(LabelSize::Small),
                                ),
                        )
                        .child(
                            div().overflow_hidden().text_ellipsis().child(
                                Label::new(entry.description.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                        ),
                ),
        )
    }
}

impl<T: PopoverTrigger> RenderOnce for ContextPicker<T> {
    fn render(self, _window: &mut gpui::Window, cx: &mut gpui::AppContext) -> impl IntoElement {
        let entries = vec![
            ContextPickerEntry {
                name: "directory".into(),
                description: "Insert any directory".into(),
                icon: IconName::Folder,
            },
            ContextPickerEntry {
                name: "file".into(),
                description: "Insert any file".into(),
                icon: IconName::File,
            },
            ContextPickerEntry {
                name: "web".into(),
                description: "Fetch content from URL".into(),
                icon: IconName::Globe,
            },
        ];

        let delegate = ContextPickerDelegate {
            all_entries: entries.clone(),
            message_editor: self.message_editor.clone(),
            filtered_entries: entries,
            selected_ix: 0,
        };

        let picker = cx.new_model(|model, cx| {
            Picker::uniform_list(delegate, model, cx).max_height(Some(rems(20.).into()))
        });

        let handle = self
            .message_editor
            .update(cx, |this, model, _| this.context_picker_handle.clone())
            .ok();
        PopoverMenu::new("context-picker")
            .menu(move |_cx| Some(picker.clone()))
            .trigger(self.trigger)
            .attach(gpui::AnchorCorner::TopLeft)
            .anchor(gpui::AnchorCorner::BottomLeft)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-16.0),
            })
            .when_some(handle, |this, handle| this.with_handle(handle))
    }
}