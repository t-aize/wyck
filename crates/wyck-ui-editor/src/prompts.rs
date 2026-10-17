//! Asking for a name, creating, duplicating and deleting scripts.

use super::{Ask, IndicatorEditor, Prompt};
use gpui::prelude::*;
use gpui::{Context, Task, Window};
use wyck_app::lost::lost;
use wyck_app::scripts as indicators;
use wyck_app::scripts::folder::LibraryError;
use wyck_indicators::custom::templates::{TEMPLATES, Template};
use wyck_ui_kit::confirm::{Tone, confirm_as};
use wyck_ui_kit::input::{InputEvent, InputState};

impl IndicatorEditor {
    /// Asks for a name for a new script made from the template `template`.
    pub fn ask_new(&mut self, template: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = TEMPLATES.get(template) else {
            return;
        };
        let name = t.name.to_owned();
        self.ask(Ask::New(template), &name, window, cx);
    }

    pub(super) fn ask_rename(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let stem = id.rsplit('/').next().unwrap_or(id).to_owned();
        self.ask(Ask::Rename(id.to_owned()), &stem, window, cx);
    }

    pub(super) fn ask(
        &mut self,
        ask: Ask,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = initial.to_owned();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, _input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.commit_prompt(window, cx);
                }
            },
        );
        input.update(cx, |state, cx| state.focus(window, cx));
        self.prompt = Some(Prompt {
            ask,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn cancel_prompt(&mut self, cx: &mut Context<Self>) {
        if self.prompt.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn commit_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let name = prompt.input.read(cx).value().to_string();
        match prompt.ask {
            Ask::New(template) => {
                let Some(Template { source, .. }) = TEMPLATES.get(template) else {
                    return;
                };
                let source = (*source).to_owned();
                let task =
                    indicators::on_library(cx, move |library| library.create(None, &name, &source));
                Self::after_create(task, window, cx);
            }
            Ask::Rename(id) => {
                let task = indicators::rename(cx, id.clone(), name);
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    lost(
                        "prompts: commit_prompt",
                        this.update_in(cx, |this, _window, cx| match result {
                            Some(Ok(new_id)) => {
                                for doc in &mut this.docs {
                                    if doc.id == id {
                                        doc.id.clone_from(&new_id);
                                    }
                                }
                                this.say(true, format!("Renamed to {new_id}."), cx);
                            }
                            Some(Err(error)) => this.say(false, error.to_string(), cx),
                            None => {}
                        }),
                    );
                })
                .detach();
            }
        }
        cx.notify();
    }

    pub(super) fn after_create(
        task: Task<Option<Result<String, LibraryError>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            lost(
                "prompts: after_create",
                this.update_in(cx, |this, window, cx| match result {
                    Some(Ok(id)) => {
                        this.open(&id, window, cx);
                        this.say(true, format!("Created {id}."), cx);
                    }
                    Some(Err(error)) => this.say(false, error.to_string(), cx),
                    None => {}
                }),
            );
        })
        .detach();
    }

    pub(super) fn duplicate(id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let id = id.to_owned();
        let task = indicators::on_library(cx, move |library| library.duplicate(&id));
        Self::after_create(task, window, cx);
    }

    /// Asks, then takes the script out of the folder (into its trash).
    pub(super) fn delete(id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (this, target) = (cx.entity(), id.to_owned());
        confirm_as(
            window,
            cx,
            Tone::Danger,
            format!("Delete {id}?"),
            "The file goes to the .trash folder inside the indicators folder, where it can still be found. Charts that hold it show it as missing.",
            move |_window, cx| {
                let this = this.clone();
                let id = target.clone();
                let task = indicators::on_library(cx, {
                    let id = id.clone();
                    move |library| library.delete(&id)
                });
                cx.spawn(async move |cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| match result {
                        Some(Ok(())) => {
                            if let Some(doc) = this.docs.iter_mut().find(|d| d.id == id) {
                                doc.gone = true;
                            }
                            this.say(true, "Deleted.", cx);
                        }
                        Some(Err(error)) => this.say(false, error.to_string(), cx),
                        None => {}
                    });
                })
                .detach();
            },
        );
    }
}
