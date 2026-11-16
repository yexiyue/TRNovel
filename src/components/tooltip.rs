//! Shared delayed, non-blocking hints; producers publish only while hovered.
use crate::theme::AppChromeTheme;
use ratatui::{
    layout::{Constraint, Margin, Offset, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use ratatui_kit::prelude::*;

#[derive(Clone, PartialEq, Eq)]
pub struct Hint {
    pub text: String,
    pub shortcut: String,
    pub anchor: Rect,
}
#[derive(Clone, Default)]
struct OwnedHint {
    owner: u64,
    content: Option<Hint>,
}
static HINT: Atom<OwnedHint> = Atom::new(OwnedHint::default);
static NEXT_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn use_tooltip(hooks: &mut Hooks, content: Option<Hint>) {
    let mut hint = hooks.use_atom(&HINT);
    let owner = hooks
        .use_state(|| NEXT_OWNER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
        .get();
    hooks.use_effect(
        {
            let content = content.clone();
            move || {
                if content.is_some() {
                    hint.set(OwnedHint { owner, content });
                } else if hint.read().owner == owner {
                    hint.set(OwnedHint::default());
                }
            }
        },
        content,
    );
    hooks.use_on_drop(move || {
        if hint.read().owner == owner {
            hint.set(OwnedHint::default());
        }
    });
}

#[derive(Default, Props)]
pub struct TooltipProps {
    pub open: bool,
    pub immediate: bool,
    pub text: String,
    pub shortcut: String,
    pub anchor: Rect,
}
#[component]
pub fn Tooltip(props: &TooltipProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let (width, height) = hooks.use_terminal_size();
    let immediate = props.immediate;
    let requested = props.open.then(|| Hint {
        text: props.text.clone(),
        shortcut: props.shortcut.clone(),
        anchor: props.anchor,
    });
    let mut shown = hooks.use_state(|| None::<Hint>);
    let mut dismissed = hooks.use_state(|| false);
    hooks.use_event_handler(EventScope::Global, EventPriority::High, move |event| {
        if immediate {
            return EventResult::Ignored;
        }
        let next = match event {
            crossterm::event::Event::Key(_) => true,
            crossterm::event::Event::Mouse(mouse) => match mouse.kind {
                crossterm::event::MouseEventKind::Moved => false,
                crossterm::event::MouseEventKind::Down(_)
                | crossterm::event::MouseEventKind::ScrollUp
                | crossterm::event::MouseEventKind::ScrollDown => true,
                _ => return EventResult::Ignored,
            },
            _ => return EventResult::Ignored,
        };
        if dismissed.get() != next {
            dismissed.set(next);
        }
        EventResult::Ignored
    });
    let suppressed = dismissed.get();
    hooks.use_async_effect(
        {
            let requested = requested.clone();
            async move {
                shown.set(None);
                if !immediate
                    && !suppressed
                    && let Some(hint) = requested
                {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    shown.set(Some(hint));
                }
            }
        },
        (requested.clone(), immediate, suppressed),
    );
    let mut spans = Vec::new();
    if !props.shortcut.is_empty() {
        spans.push(Span::styled(props.shortcut.clone(), theme.highlight));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(props.text.clone(), theme.text));
    let line = Line::from(spans);
    let popup_width = (line.width().saturating_add(4).min(56) as u16).min(width.saturating_sub(2));
    let paragraph = Paragraph::new(line).wrap(Wrap { trim: false });
    let popup_height = (paragraph.line_count(popup_width.saturating_sub(4).max(1)) as u16 + 2)
        .min(8)
        .min(height.saturating_sub(2));
    let below = props.anchor.bottom().saturating_add(1);
    let y = if below.saturating_add(popup_height) <= height {
        below
    } else {
        props.anchor.y.saturating_sub(popup_height + 1)
    };
    let visible = !suppressed
        && requested.is_some()
        && (immediate || *shown.read() == requested)
        && popup_width >= 5
        && popup_height >= 3;
    element!(Modal(open:visible,blocks_lower:Some(false),style:ratatui::style::Style::default().remove_modifier(ratatui::style::Modifier::DIM),placement:Placement::TopLeft,offset:Offset::new(i32::from(props.anchor.x.min(width.saturating_sub(popup_width))),i32::from(y)),width:Constraint::Length(popup_width),height:Constraint::Length(popup_height)) {
        Border(border_style:theme.border) { View(margin:Margin::new(1,0)) { widget(paragraph) } }
    })
}
#[component]
pub fn AppTooltip(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let hint = hooks.use_atom(&HINT).read().content.clone();
    let menu_open = super::action_menu::menu_is_open(&mut hooks);
    element!(Tooltip(open:hint.is_some() && !menu_open,text:hint.as_ref().map(|h|h.text.clone()).unwrap_or_default(),shortcut:hint.as_ref().map(|h|h.shortcut.clone()).unwrap_or_default(),anchor:hint.map(|h|h.anchor).unwrap_or_default()))
}
