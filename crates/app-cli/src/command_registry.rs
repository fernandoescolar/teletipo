//! Single source of truth for UI commands.
//!
//! `CommandId` (in `commands.rs`) is the identity of a command; this module
//! attaches everything else that used to live scattered across
//! `keybindings_ui.rs` (`BINDABLE_ACTIONS`, `DEFAULT_BINDINGS`), `commands.rs`
//! (`palette_commands`, `from_name`), and `input/pointer.rs`/`launch.rs`
//! (tab context menu labels + dispatch): the config action-name string, the
//! display label, whether it's user-bindable, its OS-default combo, whether
//! it shows in the command palette, and which context menu (if any) it
//! appears in.
//!
//! Adding a new command means adding one row here; every consumer (palette,
//! keybindings panel, config validation, context menus) picks it up
//! automatically.

use crate::commands::CommandId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandCategory {
    Tabs,
    Navigation,
    Modal,
    Clipboard,
    View,
    Dev,
    CopyMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteVisibility {
    /// Always shown in the command palette.
    Always,
    /// Only shown while an update is downloaded and ready to install.
    WhenUpdateAvailable,
    /// Never shown in the palette (bound directly to a key combo instead).
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextMenuSlot {
    /// Appears in the tab-bar right-click menu, in registry order.
    Tab,
}

pub(crate) struct CommandDef {
    pub id: CommandId,
    /// Stable snake_case name used in `config.toml`'s `[[keybindings]]` and
    /// resolved by `CommandId::from_name`.
    pub name: &'static str,
    /// Human-readable label shown in the palette and keybindings panel.
    pub label: &'static str,
    #[allow(dead_code)] // not consumed yet; reserved for future doc/menu grouping
    pub category: CommandCategory,
    pub default_binding_mac: Option<&'static str>,
    #[cfg_attr(target_os = "macos", allow(dead_code))] // only read by non-macOS builds
    pub default_binding_other: Option<&'static str>,
    /// Whether this action can be assigned a custom keybinding in the
    /// keybindings panel / `config.toml`.
    pub bindable: bool,
    pub palette: PaletteVisibility,
    pub context_menu: Option<ContextMenuSlot>,
    /// Shorter label for tight-width context menus. Falls back to `label`
    /// when `None`.
    pub context_menu_label: Option<&'static str>,
}

/// Every UI command, in canonical order (mirrors `CommandId`'s declaration
/// order). This is the single place to edit when adding a new command.
pub(crate) static COMMAND_REGISTRY: &[CommandDef] = &[
    CommandDef {
        id: CommandId::NewTab,
        name: "new_tab",
        label: "New Tab",
        category: CommandCategory::Tabs,
        default_binding_mac: Some("Cmd+T"),
        default_binding_other: Some("Ctrl+T"),
        bindable: true,
        palette: PaletteVisibility::Always,
        context_menu: Some(ContextMenuSlot::Tab),
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::CloseTab,
        name: "close_tab",
        label: "Close Tab",
        category: CommandCategory::Tabs,
        default_binding_mac: Some("Cmd+W"),
        default_binding_other: Some("Ctrl+W"),
        bindable: true,
        palette: PaletteVisibility::Always,
        context_menu: Some(ContextMenuSlot::Tab),
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::MoveTabLeft,
        name: "move_tab_left",
        label: "Move Tab Left",
        category: CommandCategory::Tabs,
        default_binding_mac: Some("Cmd+["),
        default_binding_other: Some("Ctrl+["),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: Some(ContextMenuSlot::Tab),
        context_menu_label: Some("Move Left"),
    },
    CommandDef {
        id: CommandId::MoveTabRight,
        name: "move_tab_right",
        label: "Move Tab Right",
        category: CommandCategory::Tabs,
        default_binding_mac: Some("Cmd+]"),
        default_binding_other: Some("Ctrl+]"),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: Some(ContextMenuSlot::Tab),
        context_menu_label: Some("Move Right"),
    },
    CommandDef {
        id: CommandId::JumpToPrevPrompt,
        name: "jump_to_prev_prompt",
        label: "Jump to Previous Prompt",
        category: CommandCategory::Navigation,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: true,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::JumpToNextPrompt,
        name: "jump_to_next_prompt",
        label: "Jump to Next Prompt",
        category: CommandCategory::Navigation,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: true,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::OpenSettings,
        name: "open_settings",
        label: "Open Settings",
        category: CommandCategory::Modal,
        default_binding_mac: Some("Cmd+,"),
        default_binding_other: Some("Ctrl+,"),
        bindable: true,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::OpenConfigInEditor,
        name: "open_config_in_editor",
        label: "Open Config in Editor",
        category: CommandCategory::Modal,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: true,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::RevealConfigInFinder,
        name: "reveal_config_in_finder",
        label: "Reveal Config in Finder",
        category: CommandCategory::Modal,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::RestartNow,
        name: "restart_now",
        label: "Restart Now (update ready)",
        category: CommandCategory::Modal,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::WhenUpdateAvailable,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::OpenKeybindings,
        name: "open_keybindings",
        label: "Open Keybindings",
        category: CommandCategory::Modal,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::Copy,
        name: "copy",
        label: "Copy",
        category: CommandCategory::Clipboard,
        default_binding_mac: Some("Cmd+C"),
        default_binding_other: Some("Ctrl+C"),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::Paste,
        name: "paste",
        label: "Paste",
        category: CommandCategory::Clipboard,
        default_binding_mac: Some("Cmd+V"),
        default_binding_other: Some("Ctrl+V"),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::Clear,
        name: "clear",
        label: "Clear Screen",
        category: CommandCategory::View,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::ZoomIn,
        name: "zoom_in",
        label: "Zoom In",
        category: CommandCategory::View,
        default_binding_mac: Some("Cmd++"),
        default_binding_other: Some("Ctrl++"),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::ZoomOut,
        name: "zoom_out",
        label: "Zoom Out",
        category: CommandCategory::View,
        default_binding_mac: Some("Cmd+-"),
        default_binding_other: Some("Ctrl+-"),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::OpenCommandPalette,
        name: "open_command_palette",
        label: "Open Command Palette",
        category: CommandCategory::Modal,
        default_binding_mac: Some("Cmd+Shift+P"),
        default_binding_other: Some("Ctrl+Shift+P"),
        bindable: true,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::CopyCwd,
        name: "copy_cwd",
        label: "Copy Working Directory",
        category: CommandCategory::Dev,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::OpenCwdInFinder,
        name: "open_cwd_in_finder",
        label: "Reveal Working Directory in Finder",
        category: CommandCategory::Dev,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::RepeatLastCommand,
        name: "repeat_last_command",
        label: "Repeat Last Command",
        category: CommandCategory::Dev,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::ClearScrollback,
        name: "clear_scrollback",
        label: "Clear Scrollback",
        category: CommandCategory::Dev,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::CopyLastOutput,
        name: "copy_last_output",
        label: "Copy Last Output",
        category: CommandCategory::Dev,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Always,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::CopyModeEnter,
        name: "copy_mode_enter",
        label: "Enter Copy Mode",
        category: CommandCategory::CopyMode,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
    CommandDef {
        id: CommandId::CopyModeExit,
        name: "copy_mode_exit",
        label: "Exit Copy Mode",
        category: CommandCategory::CopyMode,
        default_binding_mac: None,
        default_binding_other: None,
        bindable: false,
        palette: PaletteVisibility::Never,
        context_menu: None,
        context_menu_label: None,
    },
];

/// Look up the definition for a known `CommandId`. Every variant is guaranteed
/// to have exactly one row in `COMMAND_REGISTRY` (enforced by
/// `every_command_has_stable_name_and_label` below).
pub(crate) fn find(id: CommandId) -> &'static CommandDef {
    COMMAND_REGISTRY
        .iter()
        .find(|d| d.id == id)
        .expect("every CommandId must have a CommandDef in COMMAND_REGISTRY")
}

/// Resolve a config action-name string (case-insensitive) to its definition.
pub(crate) fn find_by_name(name: &str) -> Option<&'static CommandDef> {
    let name = name.trim().to_lowercase();
    COMMAND_REGISTRY.iter().find(|d| d.name == name)
}

/// All bindable commands, in display order for the keybindings panel.
pub(crate) fn bindable_actions() -> Vec<&'static CommandDef> {
    COMMAND_REGISTRY.iter().filter(|d| d.bindable).collect()
}

/// The OS-appropriate default combo string for `def`, if it has one.
pub(crate) fn default_binding(def: &CommandDef) -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        def.default_binding_mac
    }
    #[cfg(not(target_os = "macos"))]
    {
        def.default_binding_other
    }
}

/// Commands that appear in the tab-bar context menu, in registry order.
pub(crate) fn tab_context_menu_commands() -> Vec<&'static CommandDef> {
    COMMAND_REGISTRY
        .iter()
        .filter(|d| d.context_menu == Some(ContextMenuSlot::Tab))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_stable_name_and_label() {
        assert_eq!(COMMAND_REGISTRY.len(), 24);

        let mut seen_names = std::collections::HashSet::new();
        for def in COMMAND_REGISTRY {
            assert!(!def.label.is_empty(), "{:?} has an empty label", def.id);
            assert!(
                !def.name.is_empty()
                    && def
                        .name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{:?} name {:?} is not snake_case",
                def.id,
                def.name
            );
            assert!(
                seen_names.insert(def.name),
                "duplicate command name: {}",
                def.name
            );
            assert_eq!(
                find_by_name(def.name).map(|d| d.id),
                Some(def.id),
                "from_name round-trip failed for {}",
                def.name
            );
        }
    }

    #[test]
    fn open_keybindings_is_resolvable_by_name() {
        // Regression test: OpenKeybindings used to be missing from
        // CommandId::from_name, so it could never be bound from config.toml.
        assert_eq!(
            find_by_name("open_keybindings").map(|d| d.id),
            Some(CommandId::OpenKeybindings)
        );
    }
}
