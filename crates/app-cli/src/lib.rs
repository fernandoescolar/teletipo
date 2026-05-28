mod config;
mod coords;
mod input;
mod launch;
mod settings;
mod snapshot;
mod tab;
mod theme;
pub mod updater;
use config::UserConfig;
use launch::{build_initial_state, FontFile, load_session, save_session, spawn_pty};
use app_orchestrator::App;
use clap::Parser;
use platform_abstraction::default_shell;
use render_wgpu::{run_gpu_window_live_with_events, FontConfig, RenderConfig};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use tab::TabState;

#[derive(Debug, Parser)]
#[command(name = "teletipo", version, about = "Modern terminal/editor prototype")]
struct Cli {
    #[arg(long, default_value_t = 24)]
    rows: usize,

    #[arg(long, default_value_t = 80)]
    cols: usize,

    #[arg(long)]
    shell: Option<String>,

    #[arg(long, help = "Execute a command and exit")]
    exec: Option<String>,
}

#[derive(Debug, Default)]
struct SettingsUiState {
    open: bool,
    cursor: usize,
    edit_buf: Option<String>,
    dirty: bool,
    just_saved: bool,
}

struct GpuRuntimeState {
    tabs: Vec<TabState>,
    active_tab: usize,
    /// Shell executable used to spawn new PTY sessions.
    shell: String,
    ctrl_down: bool,
    /// Whether the Super/Command key (⌘ on macOS) is currently held.
    super_down: bool,
    window_width: u32,
    window_height: u32,
    /// Last known window top-left position in physical pixels (updated on WindowMoved).
    window_x: i32,
    window_y: i32,
    /// Current display scale factor (1.0 on standard, 2.0 on Retina, etc.).
    scale_factor: f64,
    cursor_x: f64,
    cursor_y: f64,
    /// Whether the user is currently dragging the separator bar.
    dragging_separator: bool,
    /// Whether the user is currently dragging the terminal scrollbar thumb.
    dragging_terminal_scrollbar: bool,
    /// Whether the user is currently dragging the editor scrollbar thumb.
    dragging_editor_scrollbar: bool,
    /// Time and dimensions of the last PTY resize, shown as an overlay for 1 s.
    last_resize: Option<(Instant, u16, u16)>,
    /// Whether the Shift key is currently held.
    shift_down: bool,
    /// Actual physical-pixel cell dimensions from the renderer font (updated on Resized).
    cell_w: f32,
    cell_h: f32,
    /// Tab drag-and-drop state.
    tab_drag: Option<usize>,         // index of the tab being dragged
    tab_drag_start_x: f64,           // cursor x at the moment the drag began
    /// Context menu opened by right-clicking a tab. (tab_idx, menu_x_px, menu_y_px)
    tab_context_menu: Option<(usize, f64, f64)>,
    /// Currently highlighted item inside the open context menu (0-3).
    tab_context_hover: Option<usize>,
    /// Loaded user configuration (applied per frame to the render snapshot).
    user_config: UserConfig,
    /// All theme files discovered at startup (sorted by name).
    available_themes: Vec<theme::ThemeFile>,
    /// Index into `available_themes` of the currently active preset, or `None`
    /// when the user is using custom colors.
    active_theme_idx: Option<usize>,
    /// All font files discovered at startup (index 0 = "(default)").
    available_fonts: Vec<FontFile>,
    /// Index into `available_fonts` of the currently selected font.
    /// 0 means "(default)", i.e. no font path override.
    active_font_idx: usize,
    /// Receiver for the background update-check result (consumed once after the
    /// check completes; set to `None` afterwards).
    update_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    /// Set to `Some(version)` once a newer release is detected on GitHub.
    pending_update: Option<String>,
    /// Settings overlay interaction state.
    settings: SettingsUiState,
}

impl GpuRuntimeState {
    fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Height of the tab bar in pixels (always one cell row — tab bar is always visible).
    fn tab_bar_h(&self) -> f32 {
        self.cell_h
    }

    /// Pump PTY output for ALL tabs; returns `true` if the active tab received data.
    fn pump_all_ptys(&mut self) -> bool {
        let mut active_had_data = false;
        let active = self.active_tab;
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let Some(mut pty) = tab.pty.take() else { continue };
            let had_data = tab.app.pump_pty_once(&mut pty).map(|n| n > 0).unwrap_or(false);
            tab.pty = Some(pty);
            if i == active && had_data {
                active_had_data = true;
            }
        }
        active_had_data
    }

    fn send_terminal_input(&mut self, bytes: &[u8]) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let Some(mut pty) = tab.pty.take() else { return };
        let _ = tab.app.send_pty_input(&mut pty, bytes);
        tab.pty = Some(pty);
    }

    fn run_editor_command(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let text = tab.app.editor_snapshot();
        if !text.is_empty() {
            // Deduplicate: remove earlier occurrences of the same command so the
            // most-recently-used entry always sits at the end (highest recency).
            tab.history.retain(|e| e != &text);
            // Upsert frecency entry.
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let entry_idx = tab.history_entries.iter().position(|e| e.cmd == text);
            if let Some(idx) = entry_idx {
                tab.history_entries[idx].count += 1;
                tab.history_entries[idx].last_used_secs = now_secs;
            } else {
                tab.history_entries.push(tab::HistoryEntry {
                    cmd: text.clone(),
                    count: 1,
                    last_used_secs: now_secs,
                });
            }
            tab.history.push(text);
            tab.history_index = None;
            tab.saved_input = String::new();
            // End any active Tab cycling.
            tab.suggestion_prefix = None;
            tab.suggestion_index = None;
        }
        let Some(mut pty) = tab.pty.take() else { return };
        let _ = tab.app.run_editor_command(&mut pty, false);
        tab.pty = Some(pty);
        tab.scroll_offset = 0;
        tab.editor_scroll_offset = 0;
    }

    fn history_prev(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        if tab.history.is_empty() { return; }
        let new_idx = match tab.history_index {
            None => {
                tab.saved_input = tab.app.editor_snapshot();
                tab.history.len() - 1
            }
            Some(0) => return,
            Some(i) => i - 1,
        };
        tab.history_index = Some(new_idx);
        let entry = tab.history[new_idx].clone();
        tab.app.editor_clear();
        tab.app.insert_editor_input(&entry);
        tab.editor_scroll_offset = 0;
    }

    fn history_next(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let Some(idx) = tab.history_index else { return };
        if idx + 1 < tab.history.len() {
            let new_idx = idx + 1;
            tab.history_index = Some(new_idx);
            let entry = tab.history[new_idx].clone();
            tab.app.editor_clear();
            tab.app.insert_editor_input(&entry);
            tab.editor_scroll_offset = 0;
        } else {
            tab.history_index = None;
            let saved = tab.saved_input.clone();
            tab.app.editor_clear();
            tab.app.insert_editor_input(&saved);
        }
    }

    fn resize_tab(&mut self, idx: usize, rows: u16, cols: u16) {
        let tab = &mut self.tabs[idx];
        if let Some(pty) = tab.pty.as_mut() { pty.resize(rows, cols); }
        tab.app.resize_terminal(rows as usize, cols as usize);
        let max_scroll = tab.app.scrollback_len();
        if tab.scroll_offset > max_scroll { tab.scroll_offset = max_scroll; }
    }

    /// Resize every tab after a window resize. Each tab uses its own split_ratio.
    fn resize_all_tabs(&mut self) {
        let tab_bar_h = self.tab_bar_h();
        let available_h = self.window_height as f32 - tab_bar_h;
        let pad_h = self.user_config.padding.horizontal as f32;
        let pad_v = self.user_config.padding.vertical as f32;
        let cols = ((self.window_width as f32 - 2.0 * pad_h) / self.cell_w).max(1.0) as u16;
        let n = self.tabs.len();
        for i in 0..n {
            let term_h = (available_h * self.tabs[i].split_ratio - 2.0 * pad_v).max(self.cell_h);
            let rows = (term_h / self.cell_h).max(1.0) as u16;
            self.resize_tab(i, rows, cols);
        }
    }

    fn add_new_tab(&mut self) {
        let split_ratio = self.tab().split_ratio;
        // After adding the new tab there will be at least 2 tabs, so the tab bar
        // will appear and steal one cell row — account for that when sizing the PTY.
        let tab_bar_h = self.cell_h; // will be > 1 after push
        let available_h = self.window_height as f32 - tab_bar_h;
        let pad_h = self.user_config.padding.horizontal as f32;
        let pad_v = self.user_config.padding.vertical as f32;
        let cols = ((self.window_width as f32 - 2.0 * pad_h) / self.cell_w).max(1.0) as u16;
        let term_h = (available_h * split_ratio - 2.0 * pad_v).max(self.cell_h);
        let rows = (term_h / self.cell_h).max(1.0) as u16;
        let app = App::new(rows as usize, cols as usize).expect("valid size");
        let active_cwd = self.tab().cwd.clone();
        let pty = spawn_pty(&self.shell, rows, cols, None, Some(&active_cwd)).ok();
        self.tabs.push(TabState {
            app,
            pty,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: vec![],
            history_index: None,
            saved_input: String::new(),
            split_ratio,
            selection_anchor: None,
            selection_end: None,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: rows as usize,
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            suggestion_prefix: None,
            suggestion_index: None,
            history_entries: vec![],
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Close the tab at `idx`. No-op when there is only one tab.
    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() == 1 { return; }
        self.tabs.remove(idx); // PTY is dropped → SIGHUP sent to shell
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Move the tab at `from` so that it ends up just before `insert_before`.
    /// Indices are clamped; calling with a no-op position is safe.
    fn move_tab_to(&mut self, from: usize, insert_before: usize) {
        let n = self.tabs.len();
        if from >= n { return; }
        let insert_before = insert_before.min(n);
        // No movement needed.
        if insert_before == from || insert_before == from + 1 { return; }
        let tab = self.tabs.remove(from);
        // After the remove the insertion index shifts if we are moving rightward.
        let actual = if insert_before > from { insert_before - 1 } else { insert_before };
        self.tabs.insert(actual, tab);
        // Adjust active_tab so the same tab remains active.
        if self.active_tab == from {
            self.active_tab = actual;
        } else if from < actual {
            // Tabs in (from, actual] shifted one step left.
            if self.active_tab > from && self.active_tab <= actual {
                self.active_tab -= 1;
            }
        } else {
            // Tabs in [actual, from) shifted one step right.
            if self.active_tab >= actual && self.active_tab < from {
                self.active_tab += 1;
            }
        }
    }
}


pub fn run(update_rx: std::sync::mpsc::Receiver<Option<String>>) {
    let cli = Cli::parse();
    let shell = cli.shell.unwrap_or_else(default_shell);

    // ── GPU path ─────────────────────────────────────────────────────────────
    let session = load_session();
    // Clamp to sane logical-pixel bounds — guards against session files that
    // previously stored physical pixels and grew beyond GPU texture limits.
    let window_width  = session.window_width.clamp(400, 3840);
    let window_height = session.window_height.clamp(300, 2160);
    let window_pos    = match (session.window_x, session.window_y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    let state = Rc::new(RefCell::new(build_initial_state(
        cli.rows,
        cli.cols,
        cli.exec.as_deref(),
        &shell,
        session,
        update_rx,
    )));
    let (initial_font_path, initial_font_size) = {
        let s = state.borrow();
        (s.user_config.font.path.clone(), s.user_config.font.size)
    };
    let state_for_frames = Rc::clone(&state);
    let state_for_events = Rc::clone(&state);

    if let Err(err) = run_gpu_window_live_with_events(
        move || {
            let mut state = state_for_frames.borrow_mut();
            snapshot::build_snapshot(&mut state)
        },
        move |event| {
            let mut state = state_for_events.borrow_mut();
            input::handle_event(&mut state, event);
        },
        RenderConfig {
            initial_size: Some((window_width, window_height)),
            initial_position: window_pos,
            font: FontConfig {
                font_path: initial_font_path,
                font_size: initial_font_size,
            },
            ..RenderConfig::default()
        },
    ) {
        eprintln!("failed to start gpu backend: {err}");
    }

    // Persist session state so the next run can restore it.
    save_session(&state.borrow());
}

/// Like [`suggestion_matches_tiered`] but re-sorts results by frecency score within
/// each match-quality tier.  Also prepends filesystem path completions (Tier 0)
/// when `prefix` looks like a path or starts with `cd`, man-page flag completions
/// (Tier 0b) when the user is typing flags for a known command, and appends
/// man-page command names as a last resort (Tier 4) for single-word prefixes.
pub(crate) fn suggestion_matches_frecency(
    history: &[String],
    entries: &[tab::HistoryEntry],
    prefix: &str,
    cwd: &str,
) -> Vec<String> {
    // Detect cd-command context — suppress noisy tier-2/3/4 results.
    let is_cd = prefix == "cd" || prefix.starts_with("cd ");

    // Tier 0: filesystem path completions (alphabetical).
    let t0 = path_completions(prefix, cwd);

    // Tier 0b: man-page completions for the current command.
    // • Flag tokens (starting with ‘-’): complete from man-page flags.
    // • Other tokens: complete subcommands from synopsis lines in the man page
    //   (e.g. "git co" → "git commit", "git checkout").
    let t0b: Vec<String> = if !is_cd && prefix.contains(' ') {
        let last_sp = prefix.rfind(' ').unwrap(); // safe: contains ' '
        let last_token = &prefix[last_sp + 1..];
        let base_cmd = prefix.split_whitespace().next().unwrap_or("");
        let cmd_fixed = &prefix[..=last_sp]; // "cmd " prefix including the space
        if last_token.starts_with('-') {
            man_flags(base_cmd)
                .iter()
                .filter(|flag| flag.starts_with(last_token) && flag.as_str() != last_token)
                .map(|flag| format!("{}{}", cmd_fixed, flag))
                .collect()
        } else {
            let lower_token = last_token.to_lowercase();
            // Multi-level: if 2+ tokens already typed (e.g. "git remote "),
            // prefer sub-subcommands over top-level subcommands.
            let cmd_tokens: Vec<&str> = cmd_fixed.split_whitespace().collect();
            let source_subs: Vec<String> = if cmd_tokens.len() >= 2 {
                let nested = nested_subcommands(&cmd_tokens.join(" "));
                if !nested.is_empty() { nested } else { man_subcommands(base_cmd) }
            } else {
                man_subcommands(base_cmd)
            };
            source_subs
                .iter()
                .filter(|sub| {
                    let sl = sub.to_lowercase();
                    (lower_token.is_empty() || sl.starts_with(&lower_token))
                        && sub.as_str() != last_token
                })
                .take(20)
                .map(|sub| format!("{}{}", cmd_fixed, sub))
                .collect()
        }
    } else {
        Vec::new()
    };

    // Tiers 1-2: history-based matches (strict starts-with semantics only).
    // For cd commands, suppress noisy tier-2 matches.
    let (t1, t2) = if is_cd {
        let (raw_t1, _, _) = suggestion_matches_tiered(history, prefix);
        let cd_t1: Vec<String> = raw_t1
            .into_iter()
            .filter(|e| e.starts_with("cd "))
            .collect();
        (cd_t1, Vec::new())
    } else {
        let (a, b, _) = suggestion_matches_tiered(history, prefix);
        (a, b)
    };

    // Tier 4: man-page command names — only for single-word prefixes, capped
    // at 20 to avoid flooding the dropdown.  Skipped for cd commands.
    let t4: Vec<String> = if !is_cd && !prefix.contains(' ') && !prefix.is_empty() {
        let lower = prefix.to_lowercase();
        let already: std::collections::HashSet<String> = t0
            .iter().chain(t0b.iter()).chain(t1.iter()).chain(t2.iter())
            .map(|s| s.to_lowercase())
            .collect();
        man_commands()
            .iter()
            .filter(|cmd| {
                let cl = cmd.to_lowercase();
                cl.starts_with(&lower) && cmd.as_str() != prefix && !already.contains(&cl)
            })
            .take(20)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    // Build a deduped pool from history + man suggestions.  When frecency data is
    // available the pool is sorted by score so heavily-used commands float to the top
    // regardless of whether they came from history or man pages.
    let build_pool_ordered = |t1: Vec<String>, t2: Vec<String>,
                               t0b: Vec<String>, t4: Vec<String>| -> Vec<String> {
        let mut pool: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in t1.into_iter().chain(t2).chain(t0b).chain(t4) {
            if seen.insert(item.to_lowercase()) { pool.push(item); }
        }
        pool
    };

    if entries.is_empty() {
        // No frecency data — paths first, then history + man in insertion order.
        let mut out = t0;
        out.extend(build_pool_ordered(t1, t2, t0b, t4));
        return out;
    }
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let score_map: std::collections::HashMap<&str, f32> = entries
        .iter()
        .map(|e| {
            let elapsed_hours = (now_secs.saturating_sub(e.last_used_secs)) as f32 / 3_600.0;
            (e.cmd.as_str(), e.count as f32 / (1.0 + elapsed_hours))
        })
        .collect();
    // Build a prefix-indexed score table so that man-derived suggestions like
    // "git commit" inherit the frecency of history entries like "git commit -m …".
    let prefix_scores: std::collections::HashMap<String, f32> = {
        let mut ps: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
        for (&cmd_str, &score) in &score_map {
            let tokens: Vec<&str> = cmd_str.split_whitespace().collect();
            for n in 1..=tokens.len().min(3) {
                let key = tokens[..n].join(" ");
                let e = ps.entry(key).or_insert(0.0_f32);
                *e = e.max(score);
            }
        }
        ps
    };
    let score_for = |s: &str| -> f32 {
        if let Some(&sc) = score_map.get(s) { return sc; }
        prefix_scores.get(s).copied().unwrap_or(0.0)
    };
    // Paths stay at top.  Everything else (history + man) is merged into a single
    // frecency-sorted pool.  Man suggestions inherit the score of the best matching
    // history entry that starts with them.
    let mut pool = build_pool_ordered(t1, t2, t0b, t4);
    pool.sort_by(|a, b| {
        let sa = score_for(a);
        let sb = score_for(b);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = t0;
    out.extend(pool);
    out
}

/// Tier 0: filesystem path/file completions.
///
/// Triggered when the last token of `editor_text` starts with `/`, `./`,
/// `../`, or `~`, or when the command is `cd` (completes directories from the
/// current working directory).  Returns fully-reconstructed command strings
/// sorted alphabetically.  Hidden entries are excluded unless the user typed a
/// leading `.`.
fn path_completions(editor_text: &str, cwd: &str) -> Vec<String> {
    if editor_text.is_empty() {
        return Vec::new();
    }
    let is_cd = editor_text == "cd" || editor_text.starts_with("cd ");

    // Split into the fixed command part and the path fragment being completed.
    let (cmd_part, raw_frag) = match editor_text.rfind(' ') {
        Some(pos) => (&editor_text[..=pos], &editor_text[pos + 1..]),
        None => ("", editor_text),
    };

    let is_path_like = raw_frag.starts_with('/')
        || raw_frag.starts_with("./")
        || raw_frag.starts_with("../")
        || raw_frag.starts_with('~');

    if !is_path_like && !is_cd {
        return Vec::new();
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let expanded: String = if let Some(rest) = raw_frag.strip_prefix('~') {
        format!("{}{}", home, rest)
    } else if let Some(rest) = raw_frag.strip_prefix("./") {
        format!("{}/{}", cwd.trim_end_matches('/'), rest)
    } else if let Some(rest) = raw_frag.strip_prefix("../") {
        let parent = std::path::Path::new(cwd)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        format!("{}/{}", parent.trim_end_matches('/'), rest)
    } else if raw_frag.starts_with('/') {
        raw_frag.to_string()
    } else {
        // Relative path (cd without explicit ./ prefix): resolve against cwd.
        format!("{}/{}", cwd.trim_end_matches('/'), raw_frag)
    };

    let (dir_to_read, name_prefix) = match expanded.rfind('/') {
        Some(pos) => (&expanded[..=pos], &expanded[pos + 1..]),
        None => (cwd, expanded.as_str()),
    };

    let entries = match std::fs::read_dir(dir_to_read) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    // Keep the user-typed directory sigil (e.g. "~/", "./") in the output.
    let display_dir = match raw_frag.rfind('/') {
        Some(pos) => &raw_frag[..=pos],
        None => "",
    };

    let name_lower = name_prefix.to_lowercase();
    let dirs_only = is_cd;

    let mut completions: Vec<String> = entries
        .filter_map(|res| res.ok())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Skip hidden entries unless the user explicitly typed a leading '.'.
            if name.starts_with('.') && !name_prefix.starts_with('.') {
                return false;
            }
            name.to_lowercase().starts_with(&name_lower)
        })
        .filter(|entry| {
            !dirs_only || entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
        })
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let trailer = if is_dir { "/" } else { "" };
            format!("{}{}{}{}", cmd_part, display_dir, name, trailer)
        })
        .collect();

    completions.sort();
    completions
}

/// Lazily-collected, deduplicated list of available command names from man1
/// directories and every directory in $PATH.  Populated asynchronously on first
/// call; returns an empty slice until the background scan completes.
fn man_commands() -> &'static [String] {
    static CMDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    if let Some(cmds) = CMDS.get() {
        return cmds;
    }
    static SCANNING: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    if !SCANNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        std::thread::spawn(|| {
            let _ = CMDS.set(collect_all_commands());
        });
    }
    &[]
}

/// Collects command names from man1 directories and every $PATH directory.
fn collect_all_commands() -> Vec<String> {
    let mut cmds: std::collections::HashSet<String> = std::collections::HashSet::new();

    // man1 directories.
    for dir in &[
        "/usr/share/man/man1",
        "/usr/local/share/man/man1",
        "/opt/homebrew/share/man/man1",
    ] {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name().to_string_lossy().into_owned();
            let without_gz = fname.trim_end_matches(".gz");
            let cmd = if let Some(dot) = without_gz.rfind('.') {
                &without_gz[..dot]
            } else {
                without_gz
            };
            if !cmd.is_empty() && !cmd.contains('/') {
                cmds.insert(cmd.to_string());
            }
        }
    }

    // $PATH executables — includes scripts and tools without man pages.
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(meta) = entry.metadata() {
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode() & 0o111 != 0 {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if !name.is_empty() && !name.contains('/') && !name.starts_with('.') {
                            cmds.insert(name);
                        }
                    }
                }
            }
        }
    }

    let mut v: Vec<String> = cmds.into_iter().collect();
    v.sort();
    v
}

// ── Unified asynchronous man-page data cache ─────────────────────────────────
struct ManData {
    flags: Vec<String>,
    subcommands: Vec<String>,
}

type ManDataMap = std::collections::HashMap<String, ManData>;

fn man_data_cache() -> &'static std::sync::Mutex<ManDataMap> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<ManDataMap>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Trigger asynchronous loading of man-page data for `cmd` if not yet cached.
fn ensure_man_loaded(cmd: &str) {
    if man_data_cache().lock().unwrap_or_else(|e| e.into_inner()).contains_key(cmd) {
        return;
    }
    static LOADING: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashSet<String>>,
    > = std::sync::OnceLock::new();
    let loading =
        LOADING.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    {
        let mut guard = loading.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.insert(cmd.to_string()) {
            return;
        }
    }
    let cmd_owned = cmd.to_string();
    std::thread::spawn(move || {
        let data = fetch_man_data(&cmd_owned);
        man_data_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cmd_owned.clone(), data);
        LOADING.get().unwrap()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&cmd_owned);
    });
}

fn fetch_man_data(cmd: &str) -> ManData {
    let output = std::process::Command::new("man")
        .args(["-P", "cat", cmd])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    let (flags, mut subcommands) = match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            (extract_flags_from_text(&text), extract_subcommands_from_text(cmd, &text))
        }
        Err(_) => (Vec::new(), Vec::new()),
    };
    if subcommands.is_empty() {
        subcommands = fetch_help_subcommands(cmd);
    }
    ManData { flags, subcommands }
}

fn extract_flags_from_text(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut flags: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let trimmed =
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
        if trimmed.len() < 2 || !trimmed.starts_with('-') {
            continue;
        }
        let rest = trimmed.trim_start_matches('-');
        if rest.is_empty() {
            continue;
        }
        if rest.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && seen.insert(trimmed.to_string())
        {
            flags.push(trimmed.to_string());
        }
    }
    flags.sort();
    flags
}

fn extract_subcommands_from_text(cmd: &str, text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut subs: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let rest = match trimmed
            .strip_prefix(cmd)
            .filter(|r| r.starts_with(' ') || r.starts_with('\t'))
        {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let word = rest.split_whitespace().next().unwrap_or("");
        if word.len() < 2
            || word.starts_with('-')
            || word.starts_with('[')
            || word.starts_with('<')
        {
            continue;
        }
        if word.chars().all(|c| c.is_alphanumeric() || c == '-')
            && seen.insert(word.to_string())
        {
            subs.push(word.to_string());
        }
    }
    subs.sort();
    subs
}

/// Run `<cmd> --help` and parse structured subcommand sections.
fn fetch_help_subcommands(cmd: &str) -> Vec<String> {
    let out = match std::process::Command::new(cmd)
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let text: &str = if stdout.len() >= stderr.len() { &stdout } else { &stderr };
    parse_help_text_subcommands(text)
}

/// Parse subcommand listings from `--help` output.
/// Recognises section headers containing "command"/"subcommand", then collects
/// indented lines of the form `  <word>   <description>`.
fn parse_help_text_subcommands(text: &str) -> Vec<String> {
    let mut in_section = false;
    let mut seen = std::collections::HashSet::new();
    let mut subs: Vec<String> = Vec::new();
    for line in text.lines() {
        let lower = line.to_lowercase();
        let trimmed_lower = lower.trim();
        let is_header = !line.starts_with(' ')
            && !line.starts_with('\t')
            && !line.trim().is_empty()
            && (trimmed_lower.contains("subcommand")
                || trimmed_lower == "commands"
                || trimmed_lower == "commands:"
                || trimmed_lower.starts_with("available command"));
        if is_header {
            in_section = true;
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            in_section = false;
            continue;
        }
        if !in_section || line.trim().is_empty() {
            continue;
        }
        let stripped = line.trim_start();
        let indent = line.len() - stripped.len();
        if indent < 2 {
            continue;
        }
        let word = stripped.split_whitespace().next().unwrap_or("");
        if word.len() < 2
            || word.starts_with('-')
            || word.starts_with('[')
            || word.starts_with('<')
        {
            continue;
        }
        if word.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && seen.insert(word.to_string())
        {
            subs.push(word.to_string());
        }
    }
    subs.sort();
    subs
}

fn man_flags(cmd: &str) -> Vec<String> {
    ensure_man_loaded(cmd);
    man_data_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(cmd)
        .map(|d| d.flags.clone())
        .unwrap_or_default()
}

fn man_subcommands(cmd: &str) -> Vec<String> {
    ensure_man_loaded(cmd);
    man_data_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(cmd)
        .map(|d| d.subcommands.clone())
        .unwrap_or_default()
}

/// Return subcommands for a multi-word command (e.g. "git remote").
/// Tries `man <base>-<sub>` first, then `<cmd> <sub> --help`.
/// Results are cached asynchronously — returns empty on the first call.
fn nested_subcommands(multi_cmd: &str) -> Vec<String> {
    type Cache = std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    static LOADING: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashSet<String>>,
    > = std::sync::OnceLock::new();
    fn get_cache() -> &'static Cache {
        CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
    }
    {
        let guard = get_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(subs) = guard.get(multi_cmd) {
            return subs.clone();
        }
    }
    let loading =
        LOADING.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    {
        let mut guard = loading.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.insert(multi_cmd.to_string()) {
            return Vec::new();
        }
    }
    let key = multi_cmd.to_string();
    std::thread::spawn(move || {
        let tokens: Vec<&str> = key.split_whitespace().collect();
        let subs = if tokens.len() >= 2 {
            let hyphenated = tokens.join("-");
            let man_text = std::process::Command::new("man")
                .args(["-P", "cat", &hyphenated])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            let from_man = extract_subcommands_from_text(&hyphenated, &man_text);
            if !from_man.is_empty() {
                from_man
            } else {
                match std::process::Command::new(tokens[0])
                    .args(&tokens[1..])
                    .arg("--help")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()
                {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                        let text = if stdout.len() >= stderr.len() { stdout } else { stderr };
                        parse_help_text_subcommands(&text)
                    }
                    Err(_) => Vec::new(),
                }
            }
        } else {
            Vec::new()
        };
        get_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.clone(), subs);
        LOADING.get().unwrap()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    });
    Vec::new()
}

/// Return history matches split into three quality tiers (best first):
///
/// * **Tier 1** — full prefix match (case-insensitive): the entry *starts with*
///   the typed prefix.
/// * **Tier 2** — last-token match for multi-word prefixes: the command prefix
///   matches exactly and the typed last token is a prefix of the next word
///   (e.g. `"git ch"` → `"git cherry-pick"`).  Always starts with the typed prefix.
///
/// Each tier is deduplicated and ordered most-recently-used first.  An item
/// appearing in an earlier tier is excluded from later tiers.
pub(crate) fn suggestion_matches_tiered(
    history: &[String],
    prefix: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    if prefix.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let lower = prefix.to_lowercase();

    // ── Tier 1: full prefix match ──────────────────────────────────────────
    // At most MAX_PER_NEXT_TOKEN entries per "next-token group" to prevent
    // near-identical entries (e.g. 20 "git commit -m '…'" variants) from
    // flooding the dropdown and hiding other useful completions.
    const MAX_PER_NEXT_TOKEN: usize = 3;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut next_tok_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let tier1: Vec<String> = history
        .iter()
        .rev()
        .filter(|e| {
            let el = e.to_lowercase();
            if !el.starts_with(&lower) || e.as_str() == prefix {
                return false;
            }
            if !seen.insert(el) {
                return false;
            }
            let rest = e.get(prefix.len()..).unwrap_or("").trim_start();
            let group = rest.split_whitespace().next().unwrap_or("").to_string();
            let n = next_tok_counts.entry(group).or_insert(0);
            if *n < MAX_PER_NEXT_TOKEN {
                *n += 1;
                true
            } else {
                false
            }
        })
        .cloned()
        .collect();

    // ── Tier 2: last-token match (multi-word prefix) ──────────────────────
    // The command prefix must match exactly; the final typed token must be a
    // prefix of the corresponding token in the entry.  Every result starts
    // with the full typed prefix.
    let mut tier2: Vec<String> = Vec::new();
    if let Some(space_pos) = prefix.rfind(' ') {
        let fixed_lower = prefix[..=space_pos].to_lowercase();
        let last_token = prefix[space_pos + 1..].to_lowercase();
        if !last_token.is_empty() {
            tier2 = history
                .iter()
                .rev()
                .filter(|e| {
                    let el = e.to_lowercase();
                    if seen.contains(&el) || e.as_str() == prefix {
                        return false;
                    }
                    if !el.starts_with(&fixed_lower) {
                        return false;
                    }
                    let rest = &el[fixed_lower.len()..];
                    if rest.starts_with(&last_token) {
                        seen.insert(el)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();
        }
    }

    (tier1, tier2, Vec::new())
}



