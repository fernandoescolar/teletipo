//! CLI subcommands that exit before launching the GPU runtime.
//!
//! These commands give users a way to validate or inspect their configuration
//! and themes without spinning up the renderer:
//!
//! - `teletipo config print`  — dump the loaded config (defaults applied) as TOML.
//! - `teletipo config check`  — load + validate the config and report findings.
//! - `teletipo config path`   — print the resolved config file path.
//! - `teletipo themes list`   — list bundled and user themes.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};

use crate::config::{ConfigError, UserConfig, config_path, load_config_result};
use crate::theme::{load_themes, themes_dir};
use crate::{GpuRuntimeState, UpdateBanner};

/// UI command identifiers shared by keybindings, command palette, and menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandId {
    NewTab,
    CloseTab,
    MoveTabLeft,
    MoveTabRight,
    JumpToPrevPrompt,
    JumpToNextPrompt,
    OpenSettings,
    OpenConfigInEditor,
    RevealConfigInFinder,
    RestartNow,
    OpenKeybindings,
    // Actions exposed to custom keybindings
    Copy,
    Paste,
    Clear,
    ZoomIn,
    ZoomOut,
    OpenCommandPalette,
    // Developer utilities
    CopyCwd,
    OpenCwdInFinder,
    RepeatLastCommand,
    ClearScrollback,
    CopyLastOutput,
}

impl CommandId {
    /// Map a config action-name string (snake_case) to a `CommandId`.
    pub(crate) fn from_name(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "new_tab" => Some(Self::NewTab),
            "close_tab" => Some(Self::CloseTab),
            "move_tab_left" => Some(Self::MoveTabLeft),
            "move_tab_right" => Some(Self::MoveTabRight),
            "jump_to_prev_prompt" => Some(Self::JumpToPrevPrompt),
            "jump_to_next_prompt" => Some(Self::JumpToNextPrompt),
            "open_settings" => Some(Self::OpenSettings),
            "open_config_in_editor" => Some(Self::OpenConfigInEditor),
            "reveal_config_in_finder" => Some(Self::RevealConfigInFinder),
            "restart_now" => Some(Self::RestartNow),
            "copy" => Some(Self::Copy),
            "paste" => Some(Self::Paste),
            "clear" => Some(Self::Clear),
            "zoom_in" => Some(Self::ZoomIn),
            "zoom_out" => Some(Self::ZoomOut),
            "open_command_palette" => Some(Self::OpenCommandPalette),
            "copy_cwd" => Some(Self::CopyCwd),
            "open_cwd_in_finder" => Some(Self::OpenCwdInFinder),
            "repeat_last_command" => Some(Self::RepeatLastCommand),
            "clear_scrollback" => Some(Self::ClearScrollback),
            "copy_last_output" => Some(Self::CopyLastOutput),
            _ => None,
        }
    }
}

/// Optional execution context for commands that can target a specific tab.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CommandContext {
    pub(crate) tab_idx: Option<usize>,
}

/// Execute a UI command against the live runtime state.
pub(crate) fn execute_ui_command(state: &mut GpuRuntimeState, cmd: CommandId, ctx: CommandContext) {
    let idx = ctx.tab_idx.unwrap_or(state.active_tab);
    match cmd {
        CommandId::NewTab => state.add_new_tab(),
        CommandId::CloseTab => state.close_tab(idx),
        CommandId::MoveTabLeft => state.move_tab_to(idx, idx.saturating_sub(1)),
        CommandId::MoveTabRight => state.move_tab_to(idx, idx + 2),
        CommandId::JumpToPrevPrompt => state.jump_to_prev_prompt(),
        CommandId::JumpToNextPrompt => state.jump_to_next_prompt(),
        CommandId::OpenSettings => state.open_settings_modal(),
        CommandId::OpenKeybindings => crate::keybindings_ui::open_keybindings_panel(state),
        CommandId::OpenConfigInEditor => open_config_in_editor(),
        CommandId::RevealConfigInFinder => reveal_config_in_finder(),
        CommandId::RestartNow => {
            if matches!(
                state.overlays.pending_update,
                Some(UpdateBanner::Available(_))
            ) {
                crate::updater::restart_app();
            }
        }
        CommandId::Copy => crate::input::keyboard::execute_copy(state),
        CommandId::Paste => crate::input::keyboard::execute_paste(state),
        CommandId::Clear => state.send_terminal_input(b"\x0c"),
        CommandId::ZoomIn => crate::input::keyboard::execute_zoom(state, 1.0),
        CommandId::ZoomOut => crate::input::keyboard::execute_zoom(state, -1.0),
        CommandId::OpenCommandPalette => crate::input::keyboard::open_command_palette(state),
        CommandId::CopyCwd
        | CommandId::OpenCwdInFinder
        | CommandId::RepeatLastCommand
        | CommandId::ClearScrollback
        | CommandId::CopyLastOutput => execute_dev_command(state, cmd),
    }
}

fn open_config_in_editor() {
    if let Some(path) = crate::config::config_path() {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open")
            .arg("-t")
            .arg(&path)
            .spawn();
        #[cfg(not(target_os = "macos"))]
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
    }
}

fn reveal_config_in_finder() {
    if let Some(path) = crate::config::config_path() {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open")
            .args([std::ffi::OsStr::new("-R"), path.as_os_str()])
            .spawn();
        #[cfg(not(target_os = "macos"))]
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

fn execute_dev_command(state: &mut GpuRuntimeState, cmd: CommandId) {
    match cmd {
        CommandId::CopyCwd => {
            let cwd = state.tab().cwd.clone();
            if cwd.is_empty() {
                state.push_toast("Working directory unknown", crate::state::ToastKind::Warn);
            } else {
                state.shell_services.clipboard_set(cwd);
                state.push_toast("Path copied", crate::state::ToastKind::Success);
            }
        }
        CommandId::OpenCwdInFinder => {
            let cwd = state.tab().cwd.clone();
            if !cwd.is_empty() {
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&cwd).spawn();
                #[cfg(not(target_os = "macos"))]
                let _ = std::process::Command::new("xdg-open").arg(&cwd).spawn();
            }
        }
        CommandId::RepeatLastCommand => {
            let last = state.tab().history.last().cloned();
            if let Some(cmd) = last {
                let tab = state.tab_mut();
                tab.app.editor_clear();
                tab.app.insert_editor_input(&cmd);
                tab.history_index = None;
                tab.editor_scroll_offset = 0;
                tab.editor_horizontal_scroll_offset = 0;
            }
        }
        CommandId::ClearScrollback => {
            // \x1b[3J erases saved (scrollback) lines; \x0c redraws the prompt.
            state.send_terminal_input(b"\x1b[3J\x0c");
        }
        CommandId::CopyLastOutput => {
            let output = extract_last_output(state);
            if output.is_empty() {
                state.push_toast("No command output available", crate::state::ToastKind::Warn);
            } else {
                state.shell_services.clipboard_set(output);
                state.push_toast("Output copied", crate::state::ToastKind::Success);
            }
        }
        _ => {}
    }
}

fn extract_last_output(state: &GpuRuntimeState) -> String {
    let tab = &state.tabs[state.active_tab];
    let zones = tab.app.terminal.command_zones();
    let Some(output_start) = zones.last().and_then(|z| z.output_start_row) else {
        return String::new();
    };
    let current_prompt = tab.app.terminal.current_zone_prompt_row();
    let full_text = tab.app.terminal.snapshot_text_with_scrollback();
    let total_lines = full_text.lines().count();
    let output_end = current_prompt.unwrap_or(total_lines);
    full_text
        .lines()
        .skip(output_start)
        .take(output_end.saturating_sub(output_start))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}

/// Build the stable, shared command palette entries sourced from `CommandId`.
pub(crate) fn palette_commands(state: &GpuRuntimeState) -> Vec<(String, CommandId)> {
    let mut out = vec![
        ("New Tab".to_owned(), CommandId::NewTab),
        ("Close Tab".to_owned(), CommandId::CloseTab),
        (
            "Jump to Previous Prompt".to_owned(),
            CommandId::JumpToPrevPrompt,
        ),
        (
            "Jump to Next Prompt".to_owned(),
            CommandId::JumpToNextPrompt,
        ),
        ("Open Settings".to_owned(), CommandId::OpenSettings),
        ("Open Keybindings".to_owned(), CommandId::OpenKeybindings),
        (
            "Open Config in Editor".to_owned(),
            CommandId::OpenConfigInEditor,
        ),
        (
            "Reveal Config in Finder".to_owned(),
            CommandId::RevealConfigInFinder,
        ),
    ];
    out.extend([
        ("Copy Working Directory".to_owned(), CommandId::CopyCwd),
        (
            "Reveal Working Directory in Finder".to_owned(),
            CommandId::OpenCwdInFinder,
        ),
        (
            "Repeat Last Command".to_owned(),
            CommandId::RepeatLastCommand,
        ),
        ("Clear Scrollback".to_owned(), CommandId::ClearScrollback),
        ("Copy Last Output".to_owned(), CommandId::CopyLastOutput),
    ]);

    if matches!(
        state.overlays.pending_update,
        Some(UpdateBanner::Available(_))
    ) {
        out.insert(
            0,
            (
                "Restart Now (update ready)".to_owned(),
                CommandId::RestartNow,
            ),
        );
    }
    out
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Inspect or validate the user configuration.
    Config(ConfigArgs),
    /// Inspect installed colour themes.
    Themes(ThemesArgs),
    /// Manage application updates.
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigAction {
    /// Print the loaded config (with defaults applied) as TOML.
    Print,
    /// Validate the config file and report any issues. Exits non-zero on error.
    Check,
    /// Print the absolute path of the config file.
    Path,
}

#[derive(Debug, Args)]
pub(crate) struct ThemesArgs {
    #[command(subcommand)]
    pub action: ThemesAction,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
    #[command(subcommand)]
    pub action: UpdateAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ThemesAction {
    /// List installed themes (one name per line).
    List,
}

#[derive(Debug, Subcommand)]
pub(crate) enum UpdateAction {
    /// Roll back to the most recently saved backup executable.
    Rollback,
}

/// Dispatch a parsed subcommand. Returns an `ExitCode` so `main` can exit with
/// a meaningful status (0 = OK, 1 = validation failure, 2 = missing path).
pub(crate) fn dispatch(cmd: Commands) -> ExitCode {
    match cmd {
        Commands::Config(args) => match args.action {
            ConfigAction::Print => config_print(),
            ConfigAction::Check => config_check(),
            ConfigAction::Path => config_path_cmd(),
        },
        Commands::Themes(args) => match args.action {
            ThemesAction::List => themes_list(),
        },
        Commands::Update(args) => match args.action {
            UpdateAction::Rollback => update_rollback(),
        },
    }
}

fn config_print() -> ExitCode {
    match load_config_result() {
        Ok(cfg) => match toml::to_string_pretty(&cfg) {
            Ok(s) => {
                print!("{s}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("failed to serialise config: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("failed to load config: {e}");
            ExitCode::from(1)
        }
    }
}

fn config_check() -> ExitCode {
    let path = config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".into());

    match load_config_result() {
        Ok(cfg) => {
            println!("config OK: {path}");
            print_summary(&cfg);
            ExitCode::SUCCESS
        }
        Err(ConfigError::NoConfigDir) => {
            eprintln!("error: could not determine config directory");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("config error: {e}");
            ExitCode::from(1)
        }
    }
}

fn config_path_cmd() -> ExitCode {
    match config_path() {
        Some(p) => {
            println!("{}", p.display());
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("error: could not determine config directory");
            ExitCode::from(2)
        }
    }
}

fn themes_list() -> ExitCode {
    let dir: Option<PathBuf> = themes_dir();
    let themes = load_themes();
    if themes.is_empty() {
        println!("(no themes installed)");
    } else {
        for t in &themes {
            println!("{}", t.name);
        }
    }
    if let Some(d) = dir {
        eprintln!("themes directory: {}", d.display());
    }
    ExitCode::SUCCESS
}

fn update_rollback() -> ExitCode {
    match crate::updater::rollback_latest_update() {
        Ok(true) => {
            println!("rolled back to previous executable");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            eprintln!("no rollback backup found");
            ExitCode::from(1)
        }
        Err(err) => {
            eprintln!("failed to roll back update: {err}");
            ExitCode::from(1)
        }
    }
}

fn print_summary(cfg: &UserConfig) {
    println!("  font.size          = {}", cfg.font.size);
    println!(
        "  font.family        = {}",
        cfg.font.family.as_deref().unwrap_or("(default)")
    );
    println!(
        "  padding            = {}x{}",
        cfg.padding.horizontal, cfg.padding.vertical
    );
    println!(
        "  terminal.shell     = {}",
        cfg.terminal.shell.as_deref().unwrap_or("(auto)")
    );
    println!(
        "  terminal.scrollback_lines = {}",
        cfg.terminal.scrollback_lines
    );
    println!("  terminal.bell      = {}", cfg.terminal.bell);
    println!(
        "  active_theme       = {}",
        cfg.active_theme.as_deref().unwrap_or("(none)")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_summary_handles_defaults() {
        // Smoke test: must not panic on a default config.
        print_summary(&UserConfig::default());
    }

    #[test]
    fn dispatch_themes_list_returns_success() {
        // load_themes() reads the user themes dir; whatever it returns, the
        // command must exit cleanly (success).
        let code = dispatch(Commands::Themes(ThemesArgs {
            action: ThemesAction::List,
        }));
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));
    }
}
