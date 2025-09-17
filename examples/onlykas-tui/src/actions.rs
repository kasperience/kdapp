use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

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
    Dispute,
    WatcherConfig,
    WatcherRollback,
    ChargeSub,
    ToggleList,
    None,
}

impl Action {
    pub fn from_key(key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('n') => Action::NewInvoice,
            KeyCode::Char('p') => Action::SimulatePay,
            KeyCode::Char('a') => Action::Acknowledge,
            KeyCode::Char('d') => Action::Dispute,
            KeyCode::Char('w') => Action::WatcherConfig,
            KeyCode::Char('x') => Action::WatcherRollback,
            KeyCode::Char('s') => Action::ChargeSub,
            KeyCode::Tab => Action::ToggleList,
            KeyCode::Left => Action::FocusPrev,
            KeyCode::Right => Action::FocusNext,
            KeyCode::Up => Action::SelectPrev,
            KeyCode::Down => Action::SelectNext,
            _ => Action::None,
        }
    }
}
