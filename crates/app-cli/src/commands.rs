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

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Inspect or validate the user configuration.
    Config(ConfigArgs),
    /// Inspect installed colour themes.
    Themes(ThemesArgs),
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

#[derive(Debug, Subcommand)]
pub(crate) enum ThemesAction {
    /// List installed themes (one name per line).
    List,
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
