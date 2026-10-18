//! The actions of the dashboard: what a key or a menu asks of it. They are declared apart from
//! the dashboard so that `keymap`, which binds keys to them, does not depend on the screen that
//! handles them.

#![expect(
    clippy::derive_partial_eq_without_eq,
    reason = "gpui::actions! derives PartialEq without Eq, in code that is not ours"
)]

gpui::actions!(
    wyck_dashboard,
    [
        OpenPicker,
        OpenSettings,
        ClosePicker,
        PickerUp,
        PickerDown,
        PickerPageUp,
        PickerPageDown,
        PickerConfirm,
        ToggleIndicatorEditor,
        CloseAllPositions,
        ToggleKillSwitch,
        TicketBuy,
        TicketSell,
        TicketSend,
        OpenShortcuts,
        ToggleTicket,
        ToggleAccountPanel,
        ToggleFullscreen,
        NextChart,
        PreviousChart,
        NextTimeframe,
        PreviousTimeframe,
        ToggleMagnet,
        ToggleKeepDrawing,
        ToggleFavoritesBar,
        CancelAllOrders,
        ToggleLogScale,
        TogglePercentScale,
        ToggleGrid,
        ReplayToggle,
        ReplayPlayPause,
        ReplayStepBack,
        ReplayStepForward,
        ReplayFaster,
        ReplaySlower,
    ]
);
