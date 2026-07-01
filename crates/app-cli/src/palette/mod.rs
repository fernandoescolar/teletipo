//! Command palette and provider-based item generation.
//!
//! The palette is built from multiple providers (commands, themes, fonts, shells,
//! SSH, tabs, history). Each provider is responsible for generating its own set
//! of palette items, so adding new item sources doesn't require editing the main
//! palette dispatcher.

use crate::commands::CommandId;

/// An action that can be invoked from the command palette.
#[derive(Clone, Debug)]
pub(crate) enum PaletteAction {
    Command(CommandId),
    SetTheme(usize),
    SetFont(usize),
    NewTabWithShell(String),
    /// Open a new tab running the given SSH command (e.g. `ssh user@host`).
    NewSshTab(String),
    /// Switch the palette to SSH sub-prompt mode so the user can type a destination.
    OpenSshPrompt,
    /// Switch to a different open tab by index.
    SwitchToTab(usize),
    /// Insert a command from history into the editor.
    InsertHistoryCommand(String),
    /// Fill the query with a prefix and refilter without closing the palette.
    FilterByPrefix(String),
}

/// A single item in the command palette list.
#[derive(Clone, Debug)]
pub(crate) struct PaletteItem {
    pub(crate) label: String,
    pub(crate) action: PaletteAction,
}

/// Runtime state for the command palette overlay (Cmd+Shift+P).
pub(crate) struct CommandPaletteState {
    /// Current filter query entered by the user.
    pub(crate) query: String,
    /// Byte offset of the text cursor within `query`.
    pub(crate) cursor_byte: usize,
    /// All available items (built when the palette opens and kept stable).
    pub(crate) all_items: Vec<PaletteItem>,
    /// Indices into `all_items` shown when the query is empty (primary actions +
    /// category headers). When the query is non-empty, all items are searched instead.
    pub(crate) default_filtered: Vec<usize>,
    /// Indices into `all_items` that match `query` (all items when query is empty).
    pub(crate) filtered: Vec<usize>,
    /// Index into `filtered` of the currently selected item.
    pub(crate) selected: usize,
    /// Index of the first visible item in the scroll window.
    pub(crate) scroll_offset: usize,
    /// When `Some`, the palette is in sub-prompt mode: the items list is hidden
    /// and this string is used as a label above the text input. Enter executes
    /// the action associated with the prompt (e.g. opening an SSH connection).
    pub(crate) sub_prompt: Option<SubPrompt>,
}

/// Which action to run when the user confirms a sub-prompt.
#[derive(Clone, Debug)]
pub(crate) enum SubPrompt {
    Ssh,
}

pub(crate) const PALETTE_MAX_VISIBLE: usize = 10;

impl CommandPaletteState {
    /// Re-build `filtered` from `all_items` and the current `query`.
    /// When the query is empty, shows only `default_filtered` (primary actions +
    /// category headers). When non-empty, searches across all items.
    pub(crate) fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        if q.is_empty() {
            self.filtered = self.default_filtered.clone();
        } else {
            self.filtered = (0..self.all_items.len())
                .filter(|&i| self.all_items[i].label.to_lowercase().contains(&q))
                .collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.recompute_scroll();
    }

    fn recompute_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + PALETTE_MAX_VISIBLE {
            self.scroll_offset = self.selected + 1 - PALETTE_MAX_VISIBLE;
        }
    }

    pub(crate) fn move_up(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.filtered.len() - 1;
        } else {
            self.selected -= 1;
        }
        self.recompute_scroll();
    }

    pub(crate) fn move_down(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.filtered.len();
        self.recompute_scroll();
    }
}

/// Trait for providers that contribute items to the command palette.
///
/// Each provider is responsible for a specific category of palette items
/// (commands, themes, fonts, shells, SSH, tabs, history). This trait allows
/// new item sources to be added without modifying the palette dispatcher.
pub(crate) trait PaletteProvider {
    /// Generate palette items for this provider given the current state.
    fn items(&self, ctx: &PaletteContext) -> Vec<PaletteItem>;
}

/// Context passed to palette providers containing the application state needed
/// to generate items.
pub(crate) struct PaletteContext<'a> {
    /// Reference to the runtime state.
    pub(crate) state: &'a crate::GpuRuntimeState,
    /// Index of the active tab.
    pub(crate) active_tab: usize,
}

// ── Provider implementations ──────────────────────────────────────────────

/// Provides core UI command items (New Tab, Copy, Paste, Settings, etc).
pub(crate) struct CoreCommandsProvider;

impl PaletteProvider for CoreCommandsProvider {
    fn items(&self, ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: read from command_registry::COMMAND_REGISTRY, filter by palette visibility
        Vec::new()
    }
}

/// Provides theme selection items.
pub(crate) struct ThemesProvider;

impl PaletteProvider for ThemesProvider {
    fn items(&self, _ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: read from state.themes_fonts.available_themes
        Vec::new()
    }
}

/// Provides font selection items.
pub(crate) struct FontsProvider;

impl PaletteProvider for FontsProvider {
    fn items(&self, _ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: read from state.themes_fonts.available_fonts
        Vec::new()
    }
}

/// Provides shell/terminal selection items.
pub(crate) struct ShellsProvider;

impl PaletteProvider for ShellsProvider {
    fn items(&self, _ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: read from settings::shell_options()
        Vec::new()
    }
}

/// Provides SSH host connection items.
pub(crate) struct SshProvider;

impl PaletteProvider for SshProvider {
    fn items(&self, _ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: read from state.ssh_hosts
        Vec::new()
    }
}

/// Provides tab-switching items.
pub(crate) struct TabsProvider;

impl PaletteProvider for TabsProvider {
    fn items(&self, ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: iterate state.tabs, skip active tab
        let _ = ctx;
        Vec::new()
    }
}

/// Provides command history items (frecency-ranked).
pub(crate) struct HistoryProvider;

impl PaletteProvider for HistoryProvider {
    fn items(&self, ctx: &PaletteContext) -> Vec<PaletteItem> {
        // TODO: read from state.tabs[active_tab].history_entries, cap at 100
        let _ = ctx;
        Vec::new()
    }
}

/// Get all available palette providers.
pub(crate) fn all_providers() -> Vec<Box<dyn PaletteProvider>> {
    vec![
        Box::new(CoreCommandsProvider),
        Box::new(ThemesProvider),
        Box::new(FontsProvider),
        Box::new(ShellsProvider),
        Box::new(SshProvider),
        Box::new(TabsProvider),
        Box::new(HistoryProvider),
    ]
}

/// Build all palette items from all providers.
pub(crate) fn build_all_items(ctx: &PaletteContext) -> Vec<PaletteItem> {
    all_providers()
        .iter()
        .flat_map(|provider| provider.items(ctx))
        .collect()
}

// ── Palette UI lifecycle ──────────────────────────────────────────────────

/// Open the command palette, populating it with all available items.
pub(crate) fn open(state: &mut crate::GpuRuntimeState) {
    let mut primary = build_palette_primary(state);
    primary.sort_by_key(|item| item.label.to_lowercase());

    let mut secondary = build_palette_secondary(state);
    secondary.sort_by_key(|item| item.label.to_lowercase());

    let primary_len = primary.len();
    let mut items = primary;
    items.extend(secondary);

    let default_filtered: Vec<usize> = (0..primary_len).collect();
    let filtered = default_filtered.clone();

    state.open_command_palette_modal(CommandPaletteState {
        query: String::new(),
        cursor_byte: 0,
        all_items: items,
        default_filtered,
        filtered,
        selected: 0,
        scroll_offset: 0,
        sub_prompt: None,
    });
}

fn build_palette_primary(state: &crate::GpuRuntimeState) -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> = crate::commands::palette_commands(state)
        .into_iter()
        .map(|(label, cmd)| PaletteItem {
            label,
            action: PaletteAction::Command(cmd),
        })
        .collect();

    items.push(PaletteItem {
        label: "SSH → New connection…".to_owned(),
        action: PaletteAction::OpenSshPrompt,
    });

    let active = state.active_tab;
    let headers: &[(&str, &str, bool)] = &[
        (
            "Set Theme…",
            "Set Theme: ",
            !state.themes_fonts.available_themes.is_empty(),
        ),
        (
            "Set Font…",
            "Set Font: ",
            !state.themes_fonts.available_fonts.is_empty(),
        ),
        (
            "New Tab (shell)…",
            "New Tab (",
            crate::settings::shell_options()
                .iter()
                .any(|s| s.command.is_some()),
        ),
        ("SSH → host…", "SSH → ", !state.ssh_hosts.is_empty()),
        ("Switch Tab…", "Tab ", state.tabs.len() > 1),
        (
            "History…",
            "History: ",
            !state.tabs[active].history_entries.is_empty(),
        ),
    ];
    for &(label, prefix, enabled) in headers {
        if enabled {
            items.push(PaletteItem {
                label: label.to_owned(),
                action: PaletteAction::FilterByPrefix(prefix.to_owned()),
            });
        }
    }

    items
}

fn build_palette_secondary(state: &crate::GpuRuntimeState) -> Vec<PaletteItem> {
    use std::cmp::Reverse;

    let active = state.active_tab;
    let mut items: Vec<PaletteItem> = Vec::new();

    for (i, theme) in state.themes_fonts.available_themes.iter().enumerate() {
        items.push(PaletteItem {
            label: format!("Set Theme: {}", theme.name),
            action: PaletteAction::SetTheme(i),
        });
    }
    for (i, font) in state.themes_fonts.available_fonts.iter().enumerate() {
        items.push(PaletteItem {
            label: format!("Set Font: {}", font.family),
            action: PaletteAction::SetFont(i),
        });
    }
    for shell in crate::settings::shell_options() {
        if let Some(command) = shell.command {
            items.push(PaletteItem {
                label: format!("New Tab ({})", shell.label),
                action: PaletteAction::NewTabWithShell(command),
            });
        }
    }
    for host in &state.ssh_hosts {
        items.push(PaletteItem {
            label: format!("SSH → {}", host.name),
            action: PaletteAction::NewSshTab(host.ssh_command()),
        });
    }
    for (i, tab) in state.tabs.iter().enumerate() {
        if i == active {
            continue;
        }
        let label = if tab.cwd.is_empty() {
            format!("Tab {}", i + 1)
        } else {
            format!("Tab {}: {}", i + 1, tab.cwd)
        };
        items.push(PaletteItem {
            label,
            action: PaletteAction::SwitchToTab(i),
        });
    }

    // Command history: frecency-sorted, deduped, capped at 100.
    let tab = &state.tabs[active];
    let mut history_scored: Vec<(&str, u64)> = tab
        .history_entries
        .iter()
        .map(|e| {
            let score = (e.count as u64)
                .saturating_mul(1_000_000)
                .saturating_add(e.last_used_secs);
            (e.cmd.as_str(), score)
        })
        .collect();
    history_scored.sort_by_key(|&(_, score)| Reverse(score));

    let mut seen = std::collections::HashSet::new();
    for (cmd, _) in history_scored.into_iter().take(100) {
        if seen.insert(cmd) {
            items.push(PaletteItem {
                label: format!("History: {cmd}"),
                action: PaletteAction::InsertHistoryCommand(cmd.to_owned()),
            });
        }
    }

    items
}

/// Handle keyboard input while the command palette is open.
pub(crate) fn handle_key(state: &mut crate::GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    use winit::keyboard::{Key, NamedKey};

    // Sub-prompt mode: all navigation is disabled; only text input, Enter, and Escape work.
    if state
        .command_palette
        .as_ref()
        .is_some_and(|cp| cp.sub_prompt.is_some())
    {
        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Go back to the normal palette instead of closing entirely.
                open(state);
            }
            Key::Named(NamedKey::Enter) => {
                execute_action(state);
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(cp) = state.command_palette.as_mut()
                    && let Some((byte_start, _)) =
                        cp.query[..cp.cursor_byte].char_indices().next_back()
                {
                    cp.query.remove(byte_start);
                    cp.cursor_byte = byte_start;
                }
            }
            Key::Character(ch) if !state.modifiers.super_down && !state.modifiers.ctrl_down => {
                let text = ch.as_str();
                if !text.is_empty()
                    && !text.contains('\r')
                    && !text.contains('\n')
                    && let Some(cp) = state.command_palette.as_mut()
                {
                    cp.query.insert_str(cp.cursor_byte, text);
                    cp.cursor_byte += text.len();
                }
            }
            _ => {}
        }
        return;
    }

    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => {
            state.close_active_modal();
        }
        Key::Named(NamedKey::Enter) => {
            execute_action(state);
        }
        Key::Named(NamedKey::ArrowUp) => {
            if let Some(cp) = state.command_palette.as_mut() {
                cp.move_up();
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if let Some(cp) = state.command_palette.as_mut() {
                cp.move_down();
            }
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(cp) = state.command_palette.as_mut()
                && let Some((byte_start, _)) = cp.query[..cp.cursor_byte].char_indices().next_back()
            {
                cp.query.remove(byte_start);
                cp.cursor_byte = byte_start;
                cp.refilter();
            }
        }
        Key::Character(ch) if !state.modifiers.super_down && !state.modifiers.ctrl_down => {
            let text = ch.as_str();
            if !text.is_empty()
                && !text.contains('\r')
                && !text.contains('\n')
                && let Some(cp) = state.command_palette.as_mut()
            {
                cp.query.insert_str(cp.cursor_byte, text);
                cp.cursor_byte += text.len();
                cp.refilter();
            }
        }
        _ => {}
    }
}

/// Execute the currently selected palette action, then close the palette.
/// Also callable by the pointer handler for click-to-execute.
pub(crate) fn execute_from_pointer(state: &mut crate::GpuRuntimeState) {
    execute_action(state);
}

/// Execute the currently selected palette action, then close the palette.
fn execute_action(state: &mut crate::GpuRuntimeState) {
    let Some(cp) = state.command_palette.take() else {
        return;
    };

    // Sub-prompt mode: Enter confirms the typed destination and opens a new SSH tab.
    if let Some(ref kind) = cp.sub_prompt {
        match kind {
            SubPrompt::Ssh => {
                let dest = cp.query.trim().to_owned();
                if !dest.is_empty() {
                    let cmd = format!("ssh {dest}");
                    state.add_new_tab_with_exec(&cmd);
                }
            }
        }
        // Always close the palette after confirming (modal was already taken).
        if state.overlays.active_modal == Some(crate::state::ModalOverlay::CommandPalette) {
            state.overlays.active_modal = None;
        }
        return;
    }

    let Some(&item_idx) = cp.filtered.get(cp.selected) else {
        return;
    };
    let action = cp.all_items[item_idx].action.clone();

    match action {
        PaletteAction::Command(cmd) => crate::commands::execute_ui_command(
            state,
            cmd,
            crate::commands::CommandContext::default(),
        ),
        PaletteAction::SetTheme(idx) => {
            if let Some(theme) = state.themes_fonts.available_themes.get(idx).cloned() {
                state.themes_fonts.active_theme_idx = Some(idx);
                crate::settings::apply_theme_file(&mut state.user_config, &theme);
                crate::config::save_config(&state.user_config);
            }
        }
        PaletteAction::SetFont(idx) => {
            if let Some(font) = state.themes_fonts.available_fonts.get(idx).cloned() {
                state.themes_fonts.active_font_idx = idx;
                state.user_config.font.family = if font.family == "(default)" {
                    None
                } else {
                    Some(font.family.clone())
                };
                crate::config::save_config(&state.user_config);
                state.push_toast(
                    format!("Font set to {}", font.family),
                    crate::state::ToastKind::Success,
                );
            }
        }
        PaletteAction::NewTabWithShell(shell) => {
            state.add_new_tab_with_shell(Some(shell.as_str()));
        }
        PaletteAction::NewSshTab(cmd) => {
            state.add_new_tab_with_exec(&cmd);
        }
        PaletteAction::OpenSshPrompt => {
            state.open_command_palette_modal(CommandPaletteState {
                query: String::new(),
                cursor_byte: 0,
                default_filtered: cp.default_filtered,
                all_items: cp.all_items,
                filtered: cp.filtered,
                selected: cp.selected,
                scroll_offset: cp.scroll_offset,
                sub_prompt: Some(SubPrompt::Ssh),
            });
            // Palette is now open in sub-prompt mode — do not close.
        }
        PaletteAction::SwitchToTab(idx) => {
            if idx < state.tabs.len() {
                state.active_tab = idx;
            }
        }
        PaletteAction::InsertHistoryCommand(cmd) => {
            let tab = &mut state.tabs[state.active_tab];
            tab.app.editor_clear();
            tab.app.insert_editor_input(&cmd);
            tab.history_index = None;
            tab.editor_scroll_offset = 0;
            tab.editor_horizontal_scroll_offset = 0;
        }
        PaletteAction::FilterByPrefix(prefix) => {
            // Reopen the palette with the prefix pre-filled so the user sees
            // only items from that category. The palette stays open.
            let cursor_byte = prefix.len();
            let mut new_cp = CommandPaletteState {
                query: prefix,
                cursor_byte,
                default_filtered: cp.default_filtered,
                all_items: cp.all_items,
                filtered: Vec::new(),
                selected: 0,
                scroll_offset: 0,
                sub_prompt: None,
            };
            new_cp.refilter();
            state.open_command_palette_modal(new_cp);
        }
    }
}
