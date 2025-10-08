use std::time::Duration;

use ratatui::layout::{Constraint, Flex};
use ratatui_kit::{
    AnyElement, Hooks, Props, UseFuture, UseState, component, element,
    prelude::{Border, Center, View},
};
use throbber_widgets_tui::{Throbber, ThrobberState};

use crate::hooks::UseThemeConfig;

#[derive(Debug, Clone, Default, Props)]
pub struct LoadingProps {
    pub tip: String,
}

#[component]
pub fn Loading(props: &LoadingProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let state = hooks.use_state(ThrobberState::default);

    let throbber = Throbber::default()
        .label(props.tip.clone())
        .throbber_set(throbber_widgets_tui::ASCII)
        .to_line(&state.read())
        .style(theme.loading_modal.text)
        .centered();

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            state.write().calc_next();
        }
    });

    element!(
        Center(
            width:Constraint::Percentage(50),
            height:Constraint::Length(5)
        ){
            Border(
                justify_content:Flex::Center,
                border_style:theme.loading_modal.border,
            ){
                View(height:Constraint::Length(1)){
                    $throbber
                }
            }
        }
    )
}
