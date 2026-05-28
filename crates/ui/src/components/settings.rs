//! Settings overlay state machine and shared overlay container.

use crate::actions::SettingsCmd;

use super::tabs::ContextMenuState;

#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub open: bool,
    pub cursor: usize,
    pub edit_buf: Option<String>,
    pub dirty: bool,
    pub just_saved: bool,
    pub search_buf: Option<String>,
    pub search_selected: usize,
    pub search_scroll_offset: usize,
}

impl SettingsState {
    pub fn apply(&mut self, action: SettingsCmd) {
        match action {
            SettingsCmd::MoveUp => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            SettingsCmd::MoveDown => {
                self.cursor = self.cursor.saturating_add(1);
            }
            SettingsCmd::MoveLeft | SettingsCmd::MoveRight => {}
            SettingsCmd::PageUp | SettingsCmd::Home => {
                self.cursor = 0;
                self.search_scroll_offset = 0;
            }
            SettingsCmd::PageDown | SettingsCmd::End => {
                self.cursor = self.cursor.saturating_add(10);
                self.search_scroll_offset = self.search_scroll_offset.saturating_add(10);
            }
            SettingsCmd::BeginEdit => {
                self.edit_buf = Some(String::new());
            }
            SettingsCmd::InsertChar(ch) => {
                if let Some(buf) = self.edit_buf.as_mut() {
                    buf.push(ch);
                    self.dirty = true;
                }
            }
            SettingsCmd::Backspace => {
                if let Some(buf) = self.edit_buf.as_mut() {
                    buf.pop();
                    self.dirty = true;
                }
            }
            SettingsCmd::Delete => {
                self.edit_buf = Some(String::new());
                self.dirty = true;
            }
            SettingsCmd::CommitEdit => {
                self.edit_buf = None;
            }
            SettingsCmd::CancelEdit => {
                self.edit_buf = None;
                self.dirty = false;
            }
            SettingsCmd::Save => {
                self.just_saved = true;
                self.dirty = false;
            }
            SettingsCmd::OpenSearch => {
                self.search_buf = Some(String::new());
                self.search_selected = 0;
            }
            SettingsCmd::CloseSearch => {
                self.search_buf = None;
                self.search_selected = 0;
                self.search_scroll_offset = 0;
            }
            SettingsCmd::SearchInsertChar(ch) => {
                if let Some(buf) = self.search_buf.as_mut() {
                    buf.push(ch);
                }
            }
            SettingsCmd::SearchBackspace => {
                if let Some(buf) = self.search_buf.as_mut() {
                    buf.pop();
                }
            }
            SettingsCmd::SearchNext => {
                self.search_selected = self.search_selected.saturating_add(1);
            }
            SettingsCmd::SearchPrev => {
                self.search_selected = self.search_selected.saturating_sub(1);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverlayManager {
    pub settings: SettingsState,
    pub context_menu: Option<ContextMenuState>,
}
