use crossterm::event::{KeyCode, KeyEvent};

pub enum Action {
    Quit,
    Refresh,
    FocusNext,
    FocusPrev,
    SelectNext,
    SelectPrev,
    NewInvoice,
    SimulatePay,
    Acknowledge,
    None,
}

impl Action {
    pub fn from_key(key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('n') => Action::NewInvoice,
            KeyCode::Char('p') => Action::SimulatePay,
            KeyCode::Char('a') => Action::Acknowledge,
            KeyCode::Left => Action::FocusPrev,
            KeyCode::Right => Action::FocusNext,
            KeyCode::Up => Action::SelectPrev,
            KeyCode::Down => Action::SelectNext,
            _ => Action::None,
        }
    }
}
