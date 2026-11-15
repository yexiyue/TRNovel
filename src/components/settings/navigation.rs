//! Shared keyboard transitions; hosts retain their own cursors, layers and side effects.
use super::Edit;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tabs,
    Form,
}
#[derive(Clone, Copy)]
pub struct Navigation {
    pub tab: usize,
    pub row: usize,
    pub focus: Focus,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Tab(usize),
    Row(usize),
    Focus(Focus),
    Edit(Edit),
    Close,
}

pub fn command(key: KeyEvent, state: Navigation, tabs: usize, rows: usize) -> Option<Command> {
    let last = rows.saturating_sub(1);
    let row = state.row.min(last);
    let tab = state.tab.min(tabs.saturating_sub(1));
    let shift = match key.code {
        KeyCode::Left | KeyCode::Char('h') if state.focus == Focus::Tabs => Some(-1),
        KeyCode::Right | KeyCode::Char('l') if state.focus == Focus::Tabs => Some(1),
        KeyCode::PageUp if key.modifiers.contains(KeyModifiers::CONTROL) => Some(-1),
        KeyCode::PageDown if key.modifiers.contains(KeyModifiers::CONTROL) => Some(1),
        _ => None,
    };
    if let Some(delta) = shift {
        return Some(Command::Tab(
            (tab as isize + delta).rem_euclid(tabs.max(1) as isize) as usize,
        ));
    }
    Some(match key.code {
        KeyCode::Tab | KeyCode::BackTab => Command::Focus(if state.focus == Focus::Tabs {
            Focus::Form
        } else {
            Focus::Tabs
        }),
        KeyCode::Down | KeyCode::Char('j') if state.focus == Focus::Tabs => {
            Command::Focus(Focus::Form)
        }
        KeyCode::Enter | KeyCode::Char(' ') if state.focus == Focus::Tabs => {
            Command::Focus(Focus::Form)
        }
        KeyCode::Home if state.focus == Focus::Tabs => Command::Tab(0),
        KeyCode::End if state.focus == Focus::Tabs => Command::Tab(tabs.saturating_sub(1)),
        KeyCode::Esc => Command::Close,
        _ if state.focus == Focus::Tabs => return None,
        KeyCode::Down | KeyCode::Char('j') => Command::Row((row + 1).min(last)),
        KeyCode::Up | KeyCode::Char('k') => Command::Row(row.saturating_sub(1)),
        KeyCode::Home => Command::Row(0),
        KeyCode::End => Command::Row(last),
        KeyCode::PageDown => Command::Row((row + 5).min(last)),
        KeyCode::PageUp => Command::Row(row.saturating_sub(5)),
        KeyCode::Left | KeyCode::Char('h') => Command::Edit(Edit::Adjust(row, -1)),
        KeyCode::Right | KeyCode::Char('l') => Command::Edit(Edit::Adjust(row, 1)),
        KeyCode::Enter | KeyCode::Char(' ') => Command::Edit(Edit::Activate(row)),
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn tabs_and_form_have_distinct_navigation_and_edits() {
        let state = Navigation {
            tab: 0,
            row: 3,
            focus: Focus::Tabs,
        };
        assert_eq!(
            command(key(KeyCode::Left), state, 3, 5),
            Some(Command::Tab(2))
        );
        assert_eq!(
            command(key(KeyCode::Enter), state, 3, 5),
            Some(Command::Focus(Focus::Form))
        );
        let form = Navigation {
            focus: Focus::Form,
            ..state
        };
        assert_eq!(
            command(key(KeyCode::Left), form, 3, 5),
            Some(Command::Edit(Edit::Adjust(3, -1)))
        );
        assert_eq!(
            command(key(KeyCode::PageDown), form, 3, 5),
            Some(Command::Row(4))
        );
        assert_eq!(
            command(
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL),
                form,
                3,
                5
            ),
            Some(Command::Tab(1))
        );
    }
}
