
impl Dashboard {
    /// Reads the step typed in the custom field and takes it, or says what is allowed.
    pub(super) fn replay_set_custom_step(
        &mut self,
        field: &gpui::Entity<wyck_ui_kit::input::InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = field.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(step) = parse_step(&text) else {
            toast::Toast::warning("Not a step", format!("\"{text}\" is not a step."))
                .hint(REPLAY_STEP_HELP)
                .show(cx);
            return;
        };
        if step.ticks().is_some() && !self.replay.read(cx).ticks_available() {
            toast::Toast::warning(
                "No tick data",
                "A step in ticks needs the real ticks, which this replay does not hold.",
            )
            .show(cx);
            return;
        }
        self.replay.update(cx, |replay, cx| replay.set_step(step, cx));
        field.update(cx, |state, cx| state.set_value("", window, cx));
    }
}
