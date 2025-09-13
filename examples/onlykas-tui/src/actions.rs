use crossterm::event::{KeyCode, KeyEvent};

pub enum Action {
    Quit,
    Refresh,
    FocusNext,
    FocusPrev,
    SelectNext,
    SelectPrev,
    None,
}

impl Action {
    pub fn from_key(key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Left => Action::FocusPrev,
            KeyCode::Right => Action::FocusNext,
            KeyCode::Up => Action::SelectPrev,
            KeyCode::Down => Action::SelectNext,
            _ => Action::None,
        }
    }
}
